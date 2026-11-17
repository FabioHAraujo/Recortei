//! Windows: captura via GDI, bandeja (tray-icon), tecla Insert global
//! (RegisterHotKey) e instância única (mutex + named pipe).

use std::ffi::c_void;
use std::io::Write;
use std::path::Path;
use std::ptr::null_mut;
use std::time::Duration;

use tiny_skia::{IntSize, Pixmap};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, SRCCOPY,
};
use windows_sys::Win32::Storage::FileSystem::{ReadFile, PIPE_ACCESS_INBOUND};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, WaitNamedPipeW, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::{CreateMutexW, GetCurrentProcessId};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, MOD_NOREPEAT, VK_INSERT};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetMessageW, GetSystemMetrics, MSG, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN, WM_HOTKEY,
};
use winit::event_loop::EventLoopProxy;

use crate::app::UserEvent;
use crate::draw;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---------------------------------------------------------------- captura
/// Captura a área de trabalho virtual inteira (todos os monitores), em pixels físicos.
pub fn capture_screen() -> Result<Pixmap, String> {
    unsafe {
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let w = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let h = GetSystemMetrics(SM_CYVIRTUALSCREEN);
        if w <= 0 || h <= 0 {
            return Err("tela com tamanho inválido".into());
        }
        let screen = GetDC(null_mut());
        if screen.is_null() {
            return Err("GetDC falhou".into());
        }
        let mem = CreateCompatibleDC(screen);
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp as _);
        let ok = BitBlt(mem, 0, 0, w, h, screen, x, y, SRCCOPY | CAPTUREBLT);
        SelectObject(mem, old);

        let mut bi: BITMAPINFO = std::mem::zeroed();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = w;
        bi.bmiHeader.biHeight = -h; // de cima para baixo
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = BI_RGB;
        let mut buf = vec![0u8; w as usize * h as usize * 4];
        let lines = GetDIBits(mem, bmp, 0, h as u32, buf.as_mut_ptr() as *mut c_void, &mut bi, DIB_RGB_COLORS);

        DeleteObject(bmp as _);
        DeleteDC(mem);
        ReleaseDC(null_mut(), screen);

        if ok == 0 || lines == 0 {
            return Err("BitBlt/GetDIBits falhou".into());
        }
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2); // BGRA → RGBA
            px[3] = 255;
        }
        let size = IntSize::from_wh(w as u32, h as u32).ok_or("tamanho inválido")?;
        Pixmap::from_vec(buf, size).ok_or_else(|| "buffer inválido".into())
    }
}

/// Captura síncrona; o resultado chega ao app como evento.
pub struct Capturer {
    proxy: EventLoopProxy<UserEvent>,
}

impl Capturer {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        Capturer { proxy }
    }
    pub fn request(&self) {
        let _ = self.proxy.send_event(UserEvent::Shot(capture_screen()));
    }
}

// ---------------------------------------------------------------- tecla Insert global
pub fn start_hotkey(proxy: EventLoopProxy<UserEvent>) {
    std::thread::spawn(move || unsafe {
        // hwnd nulo: WM_HOTKEY vai para a fila de mensagens desta thread
        if RegisterHotKey(null_mut(), 1, MOD_NOREPEAT, VK_INSERT as u32) == 0 {
            let _ = proxy.send_event(UserEvent::Warn(
                "Tecla Insert indisponível".into(),
                "Outro programa já usa a tecla Insert. Capture pelo ícone da bandeja.".into(),
            ));
            return;
        }
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if msg.message == WM_HOTKEY {
                let _ = proxy.send_event(UserEvent::Capture);
            }
        }
    });
}

// ---------------------------------------------------------------- bandeja
pub struct Tray {
    _icon: tray_icon::TrayIcon,
}

impl Tray {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Result<Self, String> {
        use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
        use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

        let menu = Menu::new();
        let capture = MenuItem::with_id("capture", "Capturar área (Insert)", true, None);
        let folder = MenuItem::with_id("folder", "Abrir pasta de recortes", true, None);
        let quit = MenuItem::with_id("quit", "Sair", true, None);
        menu.append_items(&[&capture, &folder, &PredefinedMenuItem::separator(), &quit])
            .map_err(|e| e.to_string())?;

        let p = std::sync::Mutex::new(proxy.clone());
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let ev = match e.id().0.as_str() {
                "capture" => UserEvent::CaptureDelayed,
                "folder" => UserEvent::OpenFolder,
                "quit" => UserEvent::Quit,
                _ => return,
            };
            if let Ok(p) = p.lock() {
                let _ = p.send_event(ev);
            }
        }));
        let p = std::sync::Mutex::new(proxy);
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                if let Ok(p) = p.lock() {
                    let _ = p.send_event(UserEvent::CaptureDelayed);
                }
            }
        }));

        let size = 32;
        let icon = Icon::from_rgba(draw::app_icon_rgba(size, true), size, size).map_err(|e| e.to_string())?;
        let tray = TrayIconBuilder::new()
            .with_id("recortei")
            .with_icon(icon)
            .with_tooltip("Recortei — Insert para capturar")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Tray { _icon: tray })
    }
}

pub fn open_folder(path: &Path) {
    if let Err(e) = std::process::Command::new("explorer.exe").arg(path).spawn() {
        eprintln!("Erro abrindo {}: {e}", path.display());
    }
}

// ---------------------------------------------------------------- instância única
pub struct Server {
    /// Handle do mutex de instância única (mantido aberto enquanto o app roda).
    _mutex: isize,
}

fn pipe_name() -> String {
    let mut session = 0u32;
    unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) };
    let user: String = std::env::var("USERNAME")
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    format!(r"\\.\pipe\io.github.recortei-{session}-{user}")
}

pub enum Instance {
    Primary(Server),
    Secondary,
}

/// Vira a instância principal ou envia `cmd` para a que já está rodando.
pub fn acquire_or_send(cmd: &str) -> Instance {
    unsafe {
        let name = wide(r"Local\io.github.recortei");
        let mutex = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if !mutex.is_null() && GetLastError() != ERROR_ALREADY_EXISTS {
            return Instance::Primary(Server { _mutex: mutex as isize });
        }
        if !mutex.is_null() {
            CloseHandle(mutex);
        }
    }
    let pipe = pipe_name();
    let wpipe = wide(&pipe);
    for _ in 0..25 {
        match std::fs::OpenOptions::new().write(true).open(&pipe) {
            Ok(mut f) => {
                let _ = f.write_all(format!("{cmd}\n").as_bytes());
                return Instance::Secondary;
            }
            Err(_) => unsafe {
                if WaitNamedPipeW(wpipe.as_ptr(), 200) == 0 {
                    std::thread::sleep(Duration::from_millis(100));
                }
            },
        }
    }
    eprintln!("Recortei já está em execução, mas não respondeu.");
    Instance::Secondary
}

impl Server {
    /// Recebe comandos de outras instâncias numa thread.
    pub fn serve(self, on_cmd: impl Fn(&str) + Send + 'static) {
        let name = wide(&pipe_name());
        std::thread::spawn(move || {
            let _keep = self;
            loop {
                let h = unsafe {
                    CreateNamedPipeW(
                        name.as_ptr(),
                        PIPE_ACCESS_INBOUND,
                        PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                        PIPE_UNLIMITED_INSTANCES,
                        0,
                        4096,
                        0,
                        std::ptr::null(),
                    )
                };
                if h == INVALID_HANDLE_VALUE {
                    std::thread::sleep(Duration::from_secs(1));
                    continue;
                }
                let connected = unsafe { ConnectNamedPipe(h, null_mut()) } != 0
                    || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
                if connected {
                    let mut data = Vec::new();
                    let mut buf = [0u8; 512];
                    loop {
                        let mut n = 0u32;
                        let ok = unsafe { ReadFile(h, buf.as_mut_ptr(), buf.len() as u32, &mut n, null_mut()) };
                        if ok == 0 || n == 0 {
                            break;
                        }
                        data.extend_from_slice(&buf[..n as usize]);
                    }
                    for line in String::from_utf8_lossy(&data).lines() {
                        on_cmd(line.trim());
                    }
                }
                unsafe {
                    DisconnectNamedPipe(h);
                    CloseHandle(h);
                }
            }
        });
    }
}

/// Mostra a saída no terminal quando o exe (subsistema "windows") é chamado de um console.
pub fn attach_console() {
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}
