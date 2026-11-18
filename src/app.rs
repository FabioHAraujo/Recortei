//! Aplicativo: fica na bandeja, recebe pedidos de captura e gerencia as
//! janelas de sobreposição (uma por monitor) sobre o screenshot congelado.

use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};

use softbuffer::{Context, Surface};
use tiny_skia::Pixmap;
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseScrollDelta, WindowEvent};
use winit::event_loop::{
    ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy, OwnedDisplayHandle,
};
use winit::keyboard::Key;
use winit::monitor::MonitorHandle;
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
use winit::window::{CursorIcon, Fullscreen, Window, WindowId, WindowLevel};

use crate::config::{save_dir, uploads_file, Config};
use crate::output::{self, Clipboard};
use crate::overlay::{make_dim_background, to_xrgb, Exit, KeyInput, Mon, Overlay, R};
use crate::platform;
use crate::upload;

#[cfg_attr(windows, allow(dead_code))]
pub const APP_ID: &str = "io.github.recortei";

/// `RECORTEI_DEBUG=1` mostra os eventos de entrada no terminal.
fn debug() -> bool {
    static D: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *D.get_or_init(|| std::env::var_os("RECORTEI_DEBUG").is_some_and(|v| v != "0"))
}

pub enum UserEvent {
    /// Tecla Insert / `recortei --capture`.
    Capture,
    /// Captura pelo menu da bandeja (espera o menu sumir).
    CaptureDelayed,
    Quit,
    OpenFolder,
    /// Outra instância foi aberta sem argumentos.
    Hello,
    Warn(String, String),
    Shot(Result<Pixmap, String>),
    /// Fim do envio para a nuvem: link (ou erro) e onde a cópia local foi salva.
    Uploaded(Result<String, String>, Result<std::path::PathBuf, String>),
}

/// Uma janela de sobreposição, cobrindo um monitor.
struct Win {
    window: Rc<Window>,
    surface: Surface<OwnedDisplayHandle, Rc<Window>>,
    /// Retângulo do monitor no layout da área de trabalho (px lógicos no
    /// Wayland, px físicos no X11/Windows).
    layout: R,
    frame: Option<Pixmap>,
    dim: Option<(Pixmap, R)>,
    shown: bool,
}

struct Session {
    ov: Overlay,
    wins: Vec<Win>,
    cursor: CursorIcon,
}

pub struct App {
    proxy: EventLoopProxy<UserEvent>,
    cfg: Config,
    capturer: platform::Capturer,
    tray: Option<platform::Tray>,
    context: Option<Context<OwnedDisplayHandle>>,
    session: Option<Session>,
    busy_since: Option<Instant>,
    capture_on_start: bool,
    clipboard: Clipboard,
    wayland: bool,
    started: bool,
}

pub fn run(server: platform::Server, capture_on_start: bool) -> Result<(), String> {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .map_err(|e| e.to_string())?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let proxy = event_loop.create_proxy();
    let p = std::sync::Mutex::new(proxy.clone());
    server.serve(move |cmd| {
        let ev = match cmd {
            "capture" => UserEvent::Capture,
            "quit" => UserEvent::Quit,
            "" => return,
            _ => UserEvent::Hello,
        };
        if let Ok(p) = p.lock() {
            let _ = p.send_event(ev);
        }
    });
    let mut app = App {
        capturer: platform::Capturer::new(proxy.clone()),
        proxy,
        cfg: Config::load(),
        tray: None,
        context: None,
        session: None,
        busy_since: None,
        capture_on_start,
        clipboard: Clipboard::default(),
        wayland: false,
        started: false,
    };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())
}

impl App {
    fn start(&mut self, el: &ActiveEventLoop) {
        if self.started {
            return;
        }
        self.started = true;
        #[cfg(target_os = "linux")]
        {
            use winit::platform::wayland::ActiveEventLoopExtWayland;
            self.wayland = el.is_wayland();
        }
        match Context::new(el.owned_display_handle()) {
            Ok(c) => self.context = Some(c),
            Err(e) => eprintln!("Erro iniciando o desenho: {e}"),
        }
        match platform::Tray::new(self.proxy.clone()) {
            Ok(t) => self.tray = Some(t),
            Err(e) => {
                eprintln!("Aviso: sem ícone na bandeja ({e})");
                let hint = if cfg!(windows) {
                    "A captura continua funcionando pela tecla Insert."
                } else {
                    "No GNOME, ative a extensão AppIndicator. A captura continua funcionando pela tecla Insert."
                };
                let _ = self.proxy.send_event(UserEvent::Warn(
                    "Ícone da bandeja indisponível".into(),
                    hint.into(),
                ));
            }
        }
        platform::start_hotkey(self.proxy.clone());
        if self.capture_on_start {
            self.request_capture();
        }
    }

    fn request_capture(&mut self) {
        if let Some(s) = &mut self.session {
            // Insert com a sobreposição aberta: confirma (ou só traz para frente)
            s.ov.insert_pressed();
            if s.ov.exit.is_none() {
                if let Some(w) = s.wins.first() {
                    w.window.focus_window();
                }
            }
            return;
        }
        if self
            .busy_since
            .is_some_and(|t| t.elapsed() < Duration::from_secs(20))
        {
            return;
        }
        self.busy_since = Some(Instant::now());
        self.capturer.request();
    }

    fn open_overlay(&mut self, el: &ActiveEventLoop, shot: Pixmap) {
        let Some(context) = &self.context else { return };
        let monitors: Vec<MonitorHandle> = el.available_monitors().collect();
        let mut wins = Vec::new();
        for m in &monitors {
            match create_window(el, context, m, self.wayland) {
                Ok(w) => wins.push(w),
                Err(e) => eprintln!("Erro criando janela: {e}"),
            }
        }
        if wins.is_empty() {
            output::notify(
                "Falha ao capturar a tela",
                "Não foi possível abrir a sobreposição.",
            );
            return;
        }
        let mons = compute_mons(&wins, shot.width(), self.wayland);
        let ov = Overlay::new(shot, mons, &self.cfg);
        self.session = Some(Session {
            ov,
            wins,
            cursor: CursorIcon::Crosshair,
        });
        let n = self.session.as_ref().map_or(0, |s| s.wins.len());
        for i in 0..n {
            if cfg!(windows) {
                // desenha antes de mostrar, para não piscar
                self.render(i);
            }
            if let Some(s) = &self.session {
                s.wins[i].window.request_redraw();
            }
        }
        if let Some(s) = &self.session {
            focus_main(&s.wins);
        }
    }

    fn render(&mut self, i: usize) {
        let Some(s) = &mut self.session else { return };
        let main = main_index(&s.wins);
        let Some(w) = s.wins.get_mut(i) else { return };
        let size = w.window.inner_size();
        let (Some(nw), Some(nh)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return;
        };
        if w.frame
            .as_ref()
            .is_none_or(|f| f.width() != size.width || f.height() != size.height)
        {
            w.frame = Pixmap::new(size.width, size.height);
        }
        let img = s.ov.mons[i].img;
        let stale = w
            .dim
            .as_ref()
            .is_none_or(|(d, r)| d.width() != size.width || d.height() != size.height || *r != img);
        if stale {
            w.dim = Some((
                make_dim_background(&s.ov.base, img, size.width, size.height),
                img,
            ));
        }
        let (Some(frame), Some((dim, _))) = (w.frame.as_mut(), w.dim.as_ref()) else {
            return;
        };
        s.ov.render_window(i, frame, dim);
        if let Err(e) = w.surface.resize(nw, nh) {
            eprintln!("Erro redimensionando superfície: {e}");
            return;
        }
        match w.surface.buffer_mut() {
            Ok(mut buf) => {
                to_xrgb(frame, &mut buf);
                if let Err(e) = buf.present() {
                    eprintln!("Erro apresentando quadro: {e}");
                }
            }
            Err(e) => eprintln!("Erro obtendo buffer: {e}"),
        }
        if !w.shown {
            w.shown = true;
            w.window.set_visible(true);
            if i == main {
                w.window.focus_window();
            }
        }
    }

    /// Depois de cada evento: fecha, redesenha e troca o cursor conforme o estado.
    fn after_input(&mut self) {
        let Some(s) = &mut self.session else { return };
        if let Some(exit) = s.ov.exit.take() {
            return self.finish(exit);
        }
        if s.ov.dirty {
            s.ov.dirty = false;
            for w in &s.wins {
                w.window.request_redraw();
            }
        }
        if s.ov.cursor != s.cursor {
            s.cursor = s.ov.cursor;
            for w in &s.wins {
                w.window.set_cursor(s.cursor);
            }
        }
    }

    fn finish(&mut self, exit: Exit) {
        let Some(mut s) = self.session.take() else {
            return;
        };
        self.cfg.remember(s.ov.tool.id(), s.ov.color, s.ov.width);
        if exit == Exit::Cancel {
            return;
        }
        let Some(pm) = s.ov.render_output() else {
            return;
        };
        if exit == Exit::Upload {
            drop(s); // fecha as janelas
            return self.start_upload(pm);
        }
        // o clipboard é definido enquanto a janela ainda tem o foco (Wayland)
        let copied = self.clipboard.set_image(&pm);
        drop(s); // fecha as janelas
        let (w, h) = (pm.width(), pm.height());
        std::thread::spawn(move || match (output::save_png(&pm), copied) {
            (Ok(path), Ok(())) => output::notify(
                "Recorte copiado",
                &format!("{w}×{h} — salvo em {}", path.display()),
            ),
            (Ok(path), Err(e)) => output::notify(
                "Recorte salvo (não copiado)",
                &format!(
                    "{w}×{h} — salvo em {}\nErro na área de transferência: {e}",
                    path.display()
                ),
            ),
            (Err(e), _) => output::notify("Erro ao salvar o recorte", &e),
        });
    }

    /// Salva a cópia local e envia ao vkdl.link numa thread; o resultado volta
    /// como `UserEvent::Uploaded` para o link ir ao clipboard no laço principal.
    fn start_upload(&mut self, pm: Pixmap) {
        let proxy = self.proxy.clone();
        let base = self.cfg.upload_url();
        let api_key = self.cfg.upload_api_key();
        output::notify(
            "Enviando recorte…",
            "Enviando para o vkdl.link — o link será copiado.",
        );
        std::thread::spawn(move || {
            let saved = output::save_png(&pm);
            let name = format!(
                "recortei-{}.png",
                chrono::Local::now().format("%Y%m%d-%H%M%S")
            );
            let result = pm
                .encode_png()
                .map_err(|e| format!("erro gerando o PNG: {e}"))
                .and_then(|png| upload::vkdl(&png, &name, &base, api_key.as_deref()))
                .map(|up| {
                    if let Err(e) = log_upload(&up) {
                        eprintln!(
                            "Erro registrando o envio em {}: {e}",
                            uploads_file().display()
                        );
                    }
                    up.url
                });
            let _ = proxy.send_event(UserEvent::Uploaded(result, saved));
        });
    }

    fn upload_done(
        &mut self,
        result: Result<String, String>,
        saved: Result<std::path::PathBuf, String>,
    ) {
        let local = match &saved {
            Ok(p) => format!("Cópia salva em {}", p.display()),
            Err(e) => format!("Cópia local não salva: {e}"),
        };
        match result {
            Ok(url) => match self.clipboard.set_text(&url) {
                Ok(()) => output::notify("Link copiado", &format!("{url}\n{local}")),
                Err(e) => output::notify(
                    "Recorte enviado (link não copiado)",
                    &format!("{url}\nErro na área de transferência: {e}\n{local}"),
                ),
            },
            Err(e) => output::notify("Falha ao enviar o recorte", &format!("{e}\n{local}")),
        }
    }

    fn window_index(&self, id: WindowId) -> Option<usize> {
        self.session
            .as_ref()?
            .wins
            .iter()
            .position(|w| w.window.id() == id)
    }

    /// Recalcula a geometria quando o tamanho/escala real das janelas é conhecido.
    fn relayout(&mut self, i: usize) {
        let wayland = self.wayland;
        let Some(s) = &mut self.session else { return };
        if wayland {
            // No Wayland a escala pode ser fracionária; o tamanho lógico real só
            // é conhecido depois que o compositor configura a janela.
            let w = &mut s.wins[i];
            let size = w.window.inner_size();
            let sf = w.window.scale_factor();
            if size.width > 0 && size.height > 0 {
                w.layout.w = (size.width as f64 / sf) as f32;
                w.layout.h = (size.height as f64 / sf) as f32;
            }
        }
        let mons = compute_mons(&s.wins, s.ov.base.width(), wayland);
        s.ov.set_mons(mons);
    }
}

fn create_window(
    el: &ActiveEventLoop,
    context: &Context<OwnedDisplayHandle>,
    m: &MonitorHandle,
    wayland: bool,
) -> Result<Win, String> {
    #[allow(unused_mut)]
    let mut attrs = Window::default_attributes()
        .with_title("Recortei")
        .with_decorations(false)
        .with_resizable(false)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_cursor(CursorIcon::Crosshair)
        .with_fullscreen(Some(Fullscreen::Borderless(Some(m.clone()))))
        .with_visible(!cfg!(windows));
    #[cfg(windows)]
    {
        use winit::platform::windows::WindowAttributesExtWindows;
        attrs = attrs
            .with_skip_taskbar(true)
            .with_position(m.position())
            .with_inner_size(m.size());
    }
    #[cfg(target_os = "linux")]
    {
        attrs = if wayland {
            winit::platform::wayland::WindowAttributesExtWayland::with_name(
                attrs, APP_ID, "recortei",
            )
        } else {
            winit::platform::x11::WindowAttributesExtX11::with_name(attrs, "recortei", "Recortei")
        };
    }
    let window = Rc::new(el.create_window(attrs).map_err(|e| e.to_string())?);
    let surface = Surface::new(context, window.clone()).map_err(|e| e.to_string())?;
    Ok(Win {
        window,
        surface,
        layout: initial_layout(m, wayland),
        frame: None,
        dim: None,
        shown: false,
    })
}

fn initial_layout(m: &MonitorHandle, wayland: bool) -> R {
    let PhysicalPosition { x, y } = m.position();
    let PhysicalSize { width, height } = m.size();
    // no Wayland o winit informa posição lógica × escala inteira do monitor
    let sf = if wayland {
        m.scale_factor().max(1.0) as f32
    } else {
        1.0
    };
    R::new(
        x as f32 / sf,
        y as f32 / sf,
        width as f32 / sf,
        height as f32 / sf,
    )
}

/// Relaciona cada monitor à região correspondente do screenshot.
///
/// O screenshot cobre o retângulo que envolve todos os monitores; `s` converte
/// unidades do layout em px da imagem (1 no Windows/X11; a escala HiDPI no Wayland).
fn compute_mons(wins: &[Win], img_w: u32, wayland: bool) -> Vec<Mon> {
    let layouts: Vec<R> = wins.iter().map(|w| w.layout).collect();
    let scales: Vec<f32> = wins
        .iter()
        .map(|w| w.window.scale_factor() as f32)
        .collect();
    mons_from_layout(&layouts, &scales, img_w, wayland)
}

pub fn mons_from_layout(layouts: &[R], scales: &[f32], img_w: u32, wayland: bool) -> Vec<Mon> {
    let minx = layouts.iter().map(|l| l.x).fold(f32::MAX, f32::min);
    let miny = layouts.iter().map(|l| l.y).fold(f32::MAX, f32::min);
    let maxx = layouts.iter().map(|l| l.right()).fold(f32::MIN, f32::max);
    let s = if maxx > minx {
        img_w as f32 / (maxx - minx)
    } else {
        1.0
    };
    layouts
        .iter()
        .zip(scales)
        .map(|(l, &sf)| Mon {
            img: R::new((l.x - minx) * s, (l.y - miny) * s, l.w * s, l.h * s),
            ui: if wayland { s } else { sf * s },
        })
        .collect()
}

/// Janela que recebe o foco: a do monitor onde está o mouse (Windows) ou a primeira.
fn main_index(wins: &[Win]) -> usize {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
        let mut p = POINT { x: 0, y: 0 };
        if unsafe { GetCursorPos(&mut p) } != 0 {
            if let Some(i) = wins
                .iter()
                .position(|w| w.layout.contains(p.x as f32, p.y as f32))
            {
                return i;
            }
        }
    }
    let _ = wins;
    0
}

fn focus_main(wins: &[Win]) {
    if let Some(w) = wins.get(main_index(wins)) {
        w.window.focus_window();
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        self.start(el);
    }

    fn user_event(&mut self, el: &ActiveEventLoop, ev: UserEvent) {
        match ev {
            UserEvent::Capture => self.request_capture(),
            UserEvent::CaptureDelayed => {
                let p = self.proxy.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(300));
                    let _ = p.send_event(UserEvent::Capture);
                });
            }
            UserEvent::Quit => el.exit(),
            UserEvent::OpenFolder => {
                let dir = save_dir();
                let _ = std::fs::create_dir_all(&dir);
                platform::open_folder(&dir);
            }
            UserEvent::Hello => output::notify(
                "O Recortei já está aberto",
                "Aperte Insert para capturar uma área da tela.",
            ),
            UserEvent::Warn(title, body) => output::notify(&title, &body),
            UserEvent::Uploaded(result, saved) => self.upload_done(result, saved),
            UserEvent::Shot(result) => {
                self.busy_since = None;
                match result {
                    Ok(pm) if self.session.is_none() => self.open_overlay(el, pm),
                    Ok(_) => {}
                    Err(e) => output::notify("Falha ao capturar a tela", &e),
                }
            }
        }
        self.after_input();
    }

    fn window_event(&mut self, _el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(i) = self.window_index(id) else {
            return;
        };
        match event {
            WindowEvent::RedrawRequested => self.render(i),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.relayout(i);
            }
            WindowEvent::CloseRequested => {
                if let Some(s) = &mut self.session {
                    s.ov.close();
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                if let Some(s) = &mut self.session {
                    s.ov.mods = m.state();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(s) = &mut self.session {
                    let m = s.ov.mons[i];
                    let k = s.wins[i].window.inner_size().width as f32 / m.img.w.max(1.0);
                    let k = if k > 0.0 { k } else { 1.0 };
                    s.ov.pointer_moved(
                        m.img.x + position.x as f32 / k,
                        m.img.y + position.y as f32 / k,
                    );
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(s) = &mut self.session {
                    if debug() {
                        eprintln!("[recortei] mouse {button:?} {state:?}");
                    }
                    match state {
                        ElementState::Pressed => s.ov.press(button),
                        ElementState::Released => s.ov.release(button),
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(s) = &mut self.session {
                    s.ov.wheel(match delta {
                        MouseScrollDelta::LineDelta(_, y) => y as f64,
                        MouseScrollDelta::PixelDelta(p) => p.y / 40.0,
                    });
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed {
                    return;
                }
                if let Some(s) = &mut self.session {
                    let ctrl = s.ov.mods.control_key();
                    // com Ctrl, usa a tecla "pura" (Ctrl+C → 'c' em qualquer layout/plataforma)
                    let logical = if ctrl {
                        event.key_without_modifiers()
                    } else {
                        event.logical_key.clone()
                    };
                    let k = KeyInput {
                        named: match &event.logical_key {
                            Key::Named(n) => Some(*n),
                            _ => None,
                        },
                        ch: match &logical {
                            Key::Character(c) => c
                                .chars()
                                .next()
                                .map(|c| c.to_lowercase().next().unwrap_or(c)),
                            _ => None,
                        },
                        text: event.text.as_ref().map(|t| t.to_string()),
                    };
                    if debug() {
                        eprintln!(
                            "[recortei] tecla {:?} texto={:?} mods={:?} -> {k:?}",
                            event.logical_key, event.text, s.ov.mods
                        );
                    }
                    s.ov.key(&k);
                }
            }
            _ => {}
        }
        self.after_input();
    }

    fn exiting(&mut self, _el: &ActiveEventLoop) {
        self.session = None;
        self.tray = None;
    }
}

/// Acrescenta uma linha em `uploads.jsonl` com o link e os dados para apagar
/// o arquivo depois (`DELETE delete_url` com `X-Delete-Token`).
fn log_upload(up: &upload::Uploaded) -> std::io::Result<()> {
    let path = uploads_file();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600); // tem os tokens de apagar
    let mut f = opts.open(path)?;
    write_record(&mut f, up, &chrono::Local::now().to_rfc3339())
}

/// Escreve o registro inteiro (JSON + `\n`) com uma única chamada `write`.
/// Com `O_APPEND`, isso evita que uploads simultâneos (threads ou processos)
/// intercalem pedaços das linhas e corrompam o `uploads.jsonl`.
fn write_record<W: std::io::Write>(
    w: &mut W,
    up: &upload::Uploaded,
    created_at: &str,
) -> std::io::Result<()> {
    let line = serde_json::json!({
        "url": up.url,
        "delete_url": up.delete_url,
        "delete_token": up.delete_token,
        "created_at": created_at,
    });
    let mut buf = serde_json::to_vec(&line)?;
    buf.push(b'\n');
    w.write_all(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_single_monitor_hidpi_wayland() {
        // monitor 4K com escala 2: layout lógico 1920×1080, screenshot 3840×2160
        let m = mons_from_layout(&[R::new(0.0, 0.0, 1920.0, 1080.0)], &[2.0], 3840, true);
        assert_eq!(m[0].img, R::new(0.0, 0.0, 3840.0, 2160.0));
        assert_eq!(m[0].ui, 2.0);
    }

    #[test]
    fn layout_two_monitors_windows() {
        // monitor secundário à esquerda (coordenadas negativas) com 150%
        let layouts = [
            R::new(0.0, 0.0, 2560.0, 1440.0),
            R::new(-1920.0, 200.0, 1920.0, 1080.0),
        ];
        let m = mons_from_layout(&layouts, &[1.0, 1.5], 4480, false);
        assert_eq!(m[0].img, R::new(1920.0, 0.0, 2560.0, 1440.0));
        assert_eq!(m[1].img, R::new(0.0, 200.0, 1920.0, 1080.0));
        assert_eq!((m[0].ui, m[1].ui), (1.0, 1.5));
    }

    /// Conta as chamadas a `write`: o registro tem que sair numa só.
    struct CountingWriter {
        calls: usize,
        data: Vec<u8>,
    }

    impl std::io::Write for CountingWriter {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.calls += 1;
            self.data.extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn sample_upload() -> upload::Uploaded {
        upload::Uploaded {
            url: "https://vkdl.link/abc123.png".into(),
            delete_url: "https://vkdl.link/api/v1/files/abc123".into(),
            delete_token: "tok\"en\\com-escape".into(),
        }
    }

    #[test]
    fn upload_record_is_written_in_one_call() {
        let mut w = CountingWriter {
            calls: 0,
            data: Vec::new(),
        };
        write_record(&mut w, &sample_upload(), "2026-11-18T10:00:00-03:00").unwrap();
        assert_eq!(w.calls, 1, "o registro deve ir numa única write()");
        assert_eq!(w.data.last(), Some(&b'\n'));
        assert_eq!(w.data.iter().filter(|&&c| c == b'\n').count(), 1);
        let v: serde_json::Value = serde_json::from_slice(&w.data).unwrap();
        assert_eq!(v["url"], "https://vkdl.link/abc123.png");
        assert_eq!(v["delete_token"], "tok\"en\\com-escape");
        assert_eq!(v["created_at"], "2026-11-18T10:00:00-03:00");
    }

    #[cfg(unix)]
    #[test]
    fn concurrent_upload_records_do_not_interleave() {
        let dir = std::env::temp_dir().join(format!("recortei-jsonl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("uploads.jsonl");
        let _ = std::fs::remove_file(&path);
        let threads: Vec<_> = (0..8)
            .map(|t| {
                let path = path.clone();
                std::thread::spawn(move || {
                    let mut up = sample_upload();
                    up.delete_token = format!("token-{t}-{}", "x".repeat(200));
                    for i in 0..200 {
                        let mut f = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&path)
                            .unwrap();
                        write_record(&mut f, &up, &format!("{t}-{i}")).unwrap();
                    }
                })
            })
            .collect();
        for h in threads {
            h.join().unwrap();
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 8 * 200);
        for l in lines {
            let v: serde_json::Value = serde_json::from_str(l).expect("linha JSONL corrompida");
            assert!(v["delete_token"].as_str().unwrap().starts_with("token-"));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
