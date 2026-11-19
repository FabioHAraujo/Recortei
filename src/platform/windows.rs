//! Windows: captura via GDI, bandeja (tray-icon), atalho global configurável
//! (RegisterHotKey), aviso da Ferramenta de Captura na tecla Print Screen e
//! instância única (mutex + named pipe).

use std::ffi::c_void;
use std::io::Write;
use std::path::Path;
use std::ptr::null_mut;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tiny_skia::{IntSize, Pixmap};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS,
    SRCCOPY,
};
use windows_sys::Win32::Storage::FileSystem::{ReadFile, PIPE_ACCESS_INBOUND};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, WaitNamedPipeW, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows_sys::Win32::System::Registry::{
    RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD, REG_SZ, RRF_RT_REG_DWORD,
    RRF_RT_REG_SZ,
};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::Threading::{
    CreateMutexW, GetCurrentProcessId, GetCurrentThreadId, OpenProcess, WaitForSingleObject,
    PROCESS_SYNCHRONIZE,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    VK_F1, VK_F24, VK_INSERT, VK_PAUSE, VK_SNAPSHOT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetMessageW, GetSystemMetrics, PeekMessageW, PostThreadMessageW, MSG, PM_NOREMOVE,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, WM_APP,
    WM_HOTKEY,
};
use winit::event_loop::EventLoopProxy;

use crate::app::UserEvent;
use crate::draw;
use crate::hotkey::{self, Hotkey, Status};

// Os códigos do hotkey.rs são os mesmos do Win32.
const _: () = {
    assert!(hotkey::MOD_ALT == MOD_ALT);
    assert!(hotkey::MOD_CONTROL == MOD_CONTROL);
    assert!(hotkey::MOD_SHIFT == MOD_SHIFT);
    assert!(hotkey::MOD_WIN == MOD_WIN);
    assert!(hotkey::MOD_NOREPEAT == MOD_NOREPEAT);
    assert!(VK_SNAPSHOT == 0x2C && VK_INSERT == 0x2D && VK_PAUSE == 0x13);
    assert!(VK_F1 == 0x70 && VK_F24 == 0x87);
};

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
        let lines = GetDIBits(
            mem,
            bmp,
            0,
            h as u32,
            buf.as_mut_ptr() as *mut c_void,
            &mut bi,
            DIB_RGB_COLORS,
        );

        DeleteObject(bmp as _);
        DeleteDC(mem);
        ReleaseDC(null_mut(), screen);

        if ok == 0 || lines == 0 {
            return Err("BitBlt/GetDIBits falhou".into());
        }
        for px in buf.as_chunks_mut::<4>().0 {
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

// ---------------------------------------------------------------- atalho global
/// Pede à thread do atalho para registrar de novo o que está em `want`.
const WM_REREGISTER: u32 = WM_APP + 1;

/// Thread com fila de mensagens que registra o atalho (hwnd nulo: o
/// `WM_HOTKEY` vai para a fila dela) e o troca na hora quando pedido.
pub struct Hotkeys {
    thread: u32,
    want: Arc<Mutex<Option<Hotkey>>>,
}

impl Hotkeys {
    pub fn start(proxy: EventLoopProxy<UserEvent>, hk: Hotkey) -> Self {
        let want = Arc::new(Mutex::new(Some(hk)));
        let (tx, rx) = std::sync::mpsc::channel();
        let w = want.clone();
        std::thread::spawn(move || unsafe {
            let mut msg: MSG = std::mem::zeroed();
            // cria a fila de mensagens antes de alguém postar nela
            PeekMessageW(&mut msg, null_mut(), WM_APP, WM_APP, PM_NOREMOVE);
            let _ = tx.send(GetCurrentThreadId());
            let mut registered = false;
            register(&w, &mut registered, &proxy);
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                match msg.message {
                    WM_HOTKEY => {
                        let _ = proxy.send_event(UserEvent::Capture);
                    }
                    WM_REREGISTER => register(&w, &mut registered, &proxy),
                    _ => {}
                }
            }
        });
        Hotkeys {
            thread: rx.recv().unwrap_or(0),
            want,
        }
    }

    fn set(&self, hk: Option<Hotkey>) {
        if let Ok(mut w) = self.want.lock() {
            *w = hk;
        }
        unsafe { PostThreadMessageW(self.thread, WM_REREGISTER, 0, 0) };
    }

    /// Troca o atalho; o resultado chega como `UserEvent::Hotkey`.
    pub fn apply(&self, hk: Hotkey) {
        self.set(Some(hk));
    }

    /// O estado já é conhecido (vem do registro).
    pub fn refresh(&self, _hk: Hotkey) {}

    /// Solta o atalho enquanto a janela de Configurações grava outro.
    pub fn pause(&self) {
        self.set(None);
    }

    pub fn resume(&self, hk: Hotkey) {
        self.set(Some(hk));
    }
}

unsafe fn register(
    want: &Mutex<Option<Hotkey>>,
    registered: &mut bool,
    proxy: &EventLoopProxy<UserEvent>,
) {
    if *registered {
        UnregisterHotKey(null_mut(), 1);
        *registered = false;
    }
    let Some(hk) = want.lock().ok().and_then(|w| *w) else {
        return;
    };
    let (mods, vk) = hk.windows();
    *registered = RegisterHotKey(null_mut(), 1, mods, vk) != 0;
    let status = if *registered {
        Status::Active
    } else {
        Status::Taken
    };
    let _ = proxy.send_event(UserEvent::Hotkey(status));
}

// ---------------------------------------------------------------- Ferramenta de Captura
/// "Usar a tecla Print Screen para abrir a captura de tela" (Windows 11).
const KEYBOARD_KEY: &str = r"Control Panel\Keyboard";
const SNIPPING_VALUE: &str = "PrintScreenKeyForSnippingEnabled";

/// A Ferramenta de Captura do Windows está com a tecla Print Screen? Valor
/// ausente conta como "não sei", não como ligado.
pub fn snipping_tool_owns_print_screen() -> bool {
    read_dword(KEYBOARD_KEY, SNIPPING_VALUE) == Some(1)
}

/// Tema dos apps no Windows (`AppsUseLightTheme`): `Some(true)` claro.
pub fn prefers_light() -> Option<bool> {
    read_dword(
        r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        "AppsUseLightTheme",
    )
    .map(|v| v != 0)
}

/// DWORD de `HKEY_CURRENT_USER\<key>`.
fn read_dword(key: &str, value: &str) -> Option<u32> {
    let (key, value) = (wide(key), wide(value));
    let mut data = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            &mut data as *mut u32 as *mut c_void,
            &mut size,
        )
    };
    (rc == 0).then_some(data)
}

/// Desliga a opção (por usuário, sem administrador). Só roda quando o
/// usuário clica em "Desativar no Windows".
pub fn disable_snipping_tool_print_screen() -> Result<(), String> {
    let (key, value) = (wide(KEYBOARD_KEY), wide(SNIPPING_VALUE));
    let zero = 0u32;
    let rc = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            REG_DWORD,
            &zero as *const u32 as *const c_void,
            std::mem::size_of::<u32>() as u32,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!("erro {rc} ao gravar no registro"))
    }
}

// ---------------------------------------------------------------- bandeja
pub struct Tray {
    icon: tray_icon::TrayIcon,
    menu: tray_icon::menu::Menu,
    capture: tray_icon::menu::MenuItem,
    /// "Reiniciar para atualizar" e o separador, quando há versão nova.
    update: Option<(
        tray_icon::menu::MenuItem,
        tray_icon::menu::PredefinedMenuItem,
    )>,
}

impl Tray {
    pub fn new(proxy: EventLoopProxy<UserEvent>, hotkey: &str) -> Result<Self, String> {
        use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
        use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

        let menu = Menu::new();
        let capture = MenuItem::with_id("capture", super::capture_item(hotkey), true, None);
        let folder = MenuItem::with_id("folder", "Abrir pasta de recortes", true, None);
        let settings = MenuItem::with_id("settings", "Configurações…", true, None);
        let quit = MenuItem::with_id("quit", "Sair", true, None);
        menu.append_items(&[
            &capture,
            &folder,
            &settings,
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .map_err(|e| e.to_string())?;

        let p = std::sync::Mutex::new(proxy.clone());
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let ev = match e.id().0.as_str() {
                "capture" => UserEvent::CaptureDelayed,
                "folder" => UserEvent::OpenFolder,
                "settings" => UserEvent::OpenSettings,
                "update" => UserEvent::Restart,
                "quit" => UserEvent::Quit,
                _ => return,
            };
            if let Ok(p) = p.lock() {
                let _ = p.send_event(ev);
            }
        }));
        let p = std::sync::Mutex::new(proxy);
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                if let Ok(p) = p.lock() {
                    let _ = p.send_event(UserEvent::CaptureDelayed);
                }
            }
        }));

        let size = 32;
        let icon = Icon::from_rgba(draw::app_icon_rgba(size, true), size, size)
            .map_err(|e| e.to_string())?;
        let tray = TrayIconBuilder::new()
            .with_id("recortei")
            .with_icon(icon)
            .with_tooltip(format!("Recortei — {}", super::tooltip(hotkey)))
            .with_menu(Box::new(menu.clone()))
            .with_menu_on_left_click(false)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Tray {
            icon: tray,
            menu,
            capture,
            update: None,
        })
    }

    /// Põe "Reiniciar para atualizar" no topo do menu.
    pub fn set_update(&mut self, label: &str) {
        use tray_icon::menu::{MenuItem, PredefinedMenuItem};
        if let Some((item, _)) = &self.update {
            item.set_text(label);
            return;
        }
        let item = MenuItem::with_id("update", label, true, None);
        let sep = PredefinedMenuItem::separator();
        if self.menu.insert_items(&[&item, &sep], 0).is_ok() {
            self.update = Some((item, sep));
        }
    }

    /// Atualiza os textos quando o atalho muda.
    pub fn set_hotkey(&self, hotkey: &str) {
        self.capture.set_text(super::capture_item(hotkey));
        let _ = self
            .icon
            .set_tooltip(Some(format!("Recortei — {}", super::tooltip(hotkey))));
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
            return Instance::Primary(Server {
                _mutex: mutex as isize,
            });
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
                        PIPE_TYPE_BYTE
                            | PIPE_READMODE_BYTE
                            | PIPE_WAIT
                            | PIPE_REJECT_REMOTE_CLIENTS,
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
                        let ok = unsafe {
                            ReadFile(h, buf.as_mut_ptr(), buf.len() as u32, &mut n, null_mut())
                        };
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

// ---------------------------------------------------------------- atualização
/// Espera o processo `pid` terminar (no máximo `timeout`).
pub fn wait_exit(pid: u32, timeout: Duration) {
    unsafe {
        let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if h.is_null() {
            return; // já terminou
        }
        WaitForSingleObject(h, timeout.as_millis().min(u32::MAX as u128) as u32);
        CloseHandle(h);
    }
}

/// Entrada do desinstalador criada pelo instalador (AppId do recortei.iss).
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\{E2A776E0-CF17-4FE6-B7E8-B8A953B0FFF4}_is1";

/// Depois de uma atualização automática, mostra a versão nova em
/// Configurações → Aplicativos (se o Recortei foi instalado nesta pasta).
pub fn set_installed_version(dir: &Path, version: &str) {
    let Some(location) = read_string(UNINSTALL_KEY, "InstallLocation") else {
        return;
    };
    let norm = |s: &str| {
        s.trim_start_matches(r"\\?\")
            .trim_end_matches(['\\', '/'])
            .to_lowercase()
    };
    if norm(&location) != norm(&dir.to_string_lossy()) {
        return;
    }
    let (key, value, data) = (wide(UNINSTALL_KEY), wide("DisplayVersion"), wide(version));
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            REG_SZ,
            data.as_ptr() as *const c_void,
            (data.len() * 2) as u32,
        );
    }
}

/// REG_SZ de `HKEY_CURRENT_USER\<key>`.
fn read_string(key: &str, value: &str) -> Option<String> {
    let (key, value) = (wide(key), wide(value));
    let mut size = 0u32;
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            null_mut(),
            &mut size,
        )
    };
    if rc != 0 || size == 0 {
        return None;
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    let rc = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr() as *mut c_void,
            &mut size,
        )
    };
    if rc != 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

/// Mostra a saída no terminal quando o exe (subsistema "windows") é chamado de um console.
pub fn attach_console() {
    use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}
