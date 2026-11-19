//! Linux: captura via xdg-desktop-portal (funciona no Wayland e no X11),
//! bandeja via StatusNotifierItem (ksni) e instância única via socket abstrato.
//! O atalho global é um atalho personalizado do GNOME que chama
//! `recortei --capture` (ver gnome.rs), pois o Wayland não deixa apps
//! capturarem teclas sozinhos.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use tiny_skia::Pixmap;
use winit::event_loop::EventLoopProxy;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedValue, Value};

use crate::app::{UserEvent, APP_ID};
use crate::draw;
use crate::gnome;
use crate::hotkey::{Hotkey, Status};

const PORTAL_DEST: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";

// ---------------------------------------------------------------- captura via portal
fn connect() -> zbus::Result<Connection> {
    let c = Connection::session()?;
    // Apps fora de sandbox se identificam ao portal (xdg-desktop-portal ≥ 1.19);
    // assim a permissão "screenshot" gravada pelo install.sh vale para nós.
    // Precisa vir antes de qualquer outra chamada ao portal nesta conexão.
    let _ = c.call_method(
        Some(PORTAL_DEST),
        PORTAL_PATH,
        Some("org.freedesktop.host.portal.Registry"),
        "Register",
        &(APP_ID, HashMap::<&str, Value>::new()),
    );
    Ok(c)
}

fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    let hex = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let rest = uri.strip_prefix("file://")?;
    let rest = rest.find('/').map(|i| &rest[i..])?; // ignora host ("file://host/...")
    Some(PathBuf::from(std::ffi::OsString::from_vec(percent_decode(
        rest,
    ))))
}

/// Tira um print da tela inteira via `org.freedesktop.portal.Screenshot`.
fn portal_screenshot(c: &Connection) -> Result<Pixmap, String> {
    let token = format!(
        "recortei{}",
        std::process::id() as u64 * 1000 + rand_u16() as u64
    );
    let sender = c
        .unique_name()
        .ok_or("conexão D-Bus sem nome")?
        .trim_start_matches(':')
        .replace('.', "_");
    let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    let request = Proxy::new(
        c,
        PORTAL_DEST,
        path.as_str(),
        "org.freedesktop.portal.Request",
    )
    .map_err(|e| e.to_string())?;
    let mut responses = request
        .receive_signal("Response")
        .map_err(|e| e.to_string())?;

    let mut opts: HashMap<&str, Value> = HashMap::new();
    opts.insert("handle_token", Value::from(token.as_str()));
    opts.insert("interactive", Value::from(false));
    c.call_method(
        Some(PORTAL_DEST),
        PORTAL_PATH,
        Some("org.freedesktop.portal.Screenshot"),
        "Screenshot",
        &("", opts),
    )
    .map_err(|e| format!("O portal de screenshot falhou: {e}"))?;

    let msg = responses.next().ok_or("o portal não respondeu")?;
    let (code, results): (u32, HashMap<String, OwnedValue>) =
        msg.body().deserialize().map_err(|e| e.to_string())?;
    if code != 0 {
        return Err("O portal de screenshot não retornou imagem (permissão negada?).".into());
    }
    let uri: String = results
        .get("uri")
        .and_then(|v| String::try_from(v.try_clone().ok()?).ok())
        .ok_or("o portal não informou o arquivo")?;
    let file = uri_to_path(&uri).ok_or("URI inválida do portal")?;
    let img = Pixmap::load_png(&file).map_err(|e| format!("Erro lendo screenshot: {e}"));
    let _ = std::fs::remove_file(&file); // o portal salva em ~/Imagens; não queremos lixo
    img
}

fn rand_u16() -> u16 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64),
    );
    h.finish() as u16
}

/// Thread com uma conexão D-Bus fixa que tira os screenshots sob demanda.
pub struct Capturer {
    tx: mpsc::Sender<()>,
}

impl Capturer {
    pub fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        let (tx, rx) = mpsc::channel::<()>();
        std::thread::spawn(move || {
            let mut conn = connect().ok();
            while rx.recv().is_ok() {
                if conn.is_none() {
                    conn = connect().ok();
                }
                let result = match &conn {
                    Some(c) => portal_screenshot(c),
                    None => Err("Sem conexão com o D-Bus da sessão.".into()),
                };
                let _ = proxy.send_event(UserEvent::Shot(result));
            }
        });
        Capturer { tx }
    }

    pub fn request(&self) {
        let _ = self.tx.send(());
    }
}

// ---------------------------------------------------------------- atalho global
#[derive(Debug, PartialEq)]
enum HotkeyReq {
    Check(Hotkey),
    Apply(Hotkey),
}

/// Thread que fala com o `gsettings` (atalho do GNOME) sem travar a janela;
/// o resultado chega como `UserEvent::Hotkey`.
pub struct Hotkeys {
    tx: mpsc::Sender<HotkeyReq>,
}

impl Hotkeys {
    /// O atalho já foi criado pelo `recortei --setup`; nada a registrar aqui.
    pub fn start(proxy: EventLoopProxy<UserEvent>, _hk: Hotkey) -> Self {
        let (tx, rx) = mpsc::channel::<HotkeyReq>();
        std::thread::spawn(move || {
            while let Ok(mut req) = rx.recv() {
                // pedidos em sequência (cliques rápidos): só o último importa,
                // mas uma conferência nunca toma o lugar de uma troca pendente
                while let Ok(next) = rx.try_recv() {
                    req = merge(req, next);
                }
                let status = match req {
                    HotkeyReq::Check(hk) => check_gnome(&hk),
                    HotkeyReq::Apply(hk) => apply_gnome(&hk),
                };
                let _ = proxy.send_event(UserEvent::Hotkey(status));
            }
        });
        Hotkeys { tx }
    }

    /// Troca a tecla do atalho do GNOME (e libera a tecla nos atalhos de
    /// captura do próprio GNOME, se for o caso).
    pub fn apply(&self, hk: Hotkey) {
        let _ = self.tx.send(HotkeyReq::Apply(hk));
    }

    /// Confere se o atalho do GNOME está com a tecla `hk`.
    pub fn refresh(&self, hk: Hotkey) {
        let _ = self.tx.send(HotkeyReq::Check(hk));
    }

    /// No GNOME o atalho é do sistema; não dá para soltá-lo durante a gravação.
    pub fn pause(&self) {}

    pub fn resume(&self, _hk: Hotkey) {}
}

/// Junta dois pedidos seguidos: vale o mais novo, a não ser que ele seja só
/// uma conferência e o anterior uma troca (o `Apply` já informa a situação).
fn merge(req: HotkeyReq, next: HotkeyReq) -> HotkeyReq {
    match (req, next) {
        (HotkeyReq::Apply(a), HotkeyReq::Check(_)) => HotkeyReq::Apply(a),
        (_, n) => n,
    }
}

fn check_gnome(hk: &Hotkey) -> Status {
    if !gnome::available() {
        return Status::Manual;
    }
    match gnome::custom_binding() {
        Some(b) if Hotkey::from_gnome(&b) == Some(*hk) => Status::Active,
        Some(b) => Status::Error(format!(
            "O atalho do GNOME está com outra tecla ({b}). Escolha um atalho acima para corrigir."
        )),
        None => Status::Error(
            "O atalho do GNOME ainda não foi criado. Escolha um atalho acima para criá-lo.".into(),
        ),
    }
}

fn apply_gnome(hk: &Hotkey) -> Status {
    if !gnome::available() {
        return Status::Manual;
    }
    match gnome::bind(Some(hk), None) {
        Ok(_) => Status::Active,
        Err(e) => Status::Error(format!("Não foi possível aplicar o atalho no GNOME: {e}.")),
    }
}

/// A Ferramenta de Captura do Windows não existe aqui.
pub fn snipping_tool_owns_print_screen() -> bool {
    false
}

pub fn disable_snipping_tool_print_screen() -> Result<(), String> {
    Err("disponível só no Windows".into())
}

/// Tema do sistema pelo portal (`org.freedesktop.appearance color-scheme`):
/// `Some(true)` claro, `Some(false)` escuro, `None` se não deu para saber.
pub fn prefers_light() -> Option<bool> {
    let c = Connection::session().ok()?;
    // ReadOne é do portal ≥ 1.15; o Read antigo devolve a variante dentro de outra
    let scheme = ["ReadOne", "Read"].iter().find_map(|method| {
        let reply = c
            .call_method(
                Some(PORTAL_DEST),
                PORTAL_PATH,
                Some("org.freedesktop.portal.Settings"),
                *method,
                &("org.freedesktop.appearance", "color-scheme"),
            )
            .ok()?;
        let v: OwnedValue = reply.body().deserialize().ok()?;
        variant_u32(&v)
    })?;
    // 1 = escuro; 0 (sem preferência) e 2 = claro, como o GNOME/libadwaita
    Some(scheme != 1)
}

fn variant_u32(v: &Value) -> Option<u32> {
    match v {
        Value::U32(n) => Some(*n),
        Value::Value(inner) => variant_u32(inner),
        _ => None,
    }
}

// ---------------------------------------------------------------- bandeja
struct SniTray {
    proxy: EventLoopProxy<UserEvent>,
    hotkey: String,
    /// "Reiniciar para atualizar (x.y.z)", quando há versão nova.
    update: Option<String>,
}

impl SniTray {
    fn send(&self, ev: UserEvent) {
        let _ = self.proxy.send_event(ev);
    }
}

fn argb_icon(size: u32) -> ksni::Icon {
    let rgba = draw::app_icon_rgba(size, false);
    let mut data = Vec::with_capacity(rgba.len());
    for px in rgba.as_chunks::<4>().0 {
        data.extend_from_slice(&[px[3], px[0], px[1], px[2]]);
    }
    ksni::Icon {
        width: size as i32,
        height: size as i32,
        data,
    }
}

impl ksni::Tray for SniTray {
    fn id(&self) -> String {
        "recortei".into()
    }
    fn title(&self) -> String {
        "Recortei".into()
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![argb_icon(22), argb_icon(44)]
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "Recortei".into(),
            description: super::tooltip(&self.hotkey),
            ..Default::default()
        }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(UserEvent::CaptureDelayed);
    }
    fn secondary_activate(&mut self, _x: i32, _y: i32) {
        self.send(UserEvent::CaptureDelayed);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{MenuItem, StandardItem};
        let item = |label: &str, ev: fn() -> UserEvent| -> MenuItem<Self> {
            StandardItem {
                label: label.into(),
                activate: Box::new(move |t: &mut Self| t.send(ev())),
                ..Default::default()
            }
            .into()
        };
        let mut menu = Vec::new();
        if let Some(label) = &self.update {
            menu.push(item(label, || UserEvent::Restart));
            menu.push(MenuItem::Separator);
        }
        menu.extend([
            item(&super::capture_item(&self.hotkey), || {
                UserEvent::CaptureDelayed
            }),
            item("Abrir pasta de recortes", || UserEvent::OpenFolder),
            item("Configurações…", || UserEvent::OpenSettings),
            MenuItem::Separator,
            item("Sair", || UserEvent::Quit),
        ]);
        menu
    }
}

pub struct Tray {
    handle: ksni::blocking::Handle<SniTray>,
}

impl Tray {
    pub fn new(proxy: EventLoopProxy<UserEvent>, hotkey: &str) -> Result<Self, String> {
        use ksni::blocking::TrayMethods;
        let tray = SniTray {
            proxy,
            hotkey: hotkey.into(),
            update: None,
        };
        let handle = tray.spawn().map_err(|e| e.to_string())?;
        Ok(Tray { handle })
    }

    /// Atualiza os textos quando o atalho muda.
    pub fn set_hotkey(&self, hotkey: &str) {
        self.handle.update(|t| t.hotkey = hotkey.into());
    }

    /// Põe "Reiniciar para atualizar" no topo do menu.
    pub fn set_update(&mut self, label: &str) {
        self.handle.update(|t| t.update = Some(label.into()));
    }
}

pub fn open_folder(path: &Path) {
    for cmd in ["xdg-open", "gio"] {
        let mut c = std::process::Command::new(cmd);
        if cmd == "gio" {
            c.arg("open");
        }
        if c.arg(path).spawn().is_ok() {
            return;
        }
    }
    eprintln!("Não foi possível abrir {}", path.display());
}

// ---------------------------------------------------------------- instância única
fn socket_addr() -> std::io::Result<SocketAddr> {
    use std::os::unix::fs::MetadataExt;
    let uid = std::fs::metadata("/proc/self")
        .map(|m| m.uid())
        .unwrap_or(0);
    SocketAddr::from_abstract_name(format!("{APP_ID}.{uid}").as_bytes())
}

pub struct Server {
    listener: Option<UnixListener>,
}

pub enum Instance {
    Primary(Server),
    Secondary,
}

fn send(addr: &SocketAddr, cmd: &str) -> bool {
    match UnixStream::connect_addr(addr) {
        Ok(mut s) => {
            let _ = s.set_write_timeout(Some(Duration::from_secs(2)));
            let _ = s.write_all(format!("{cmd}\n").as_bytes());
            true
        }
        Err(_) => false,
    }
}

/// Vira a instância principal ou envia `cmd` para a que já está rodando.
pub fn acquire_or_send(cmd: &str) -> Instance {
    let Ok(addr) = socket_addr() else {
        return Instance::Primary(Server { listener: None });
    };
    if send(&addr, cmd) {
        return Instance::Secondary;
    }
    match UnixListener::bind_addr(&addr) {
        Ok(l) => Instance::Primary(Server { listener: Some(l) }),
        // outra instância acabou de subir
        Err(_) if send(&addr, cmd) => Instance::Secondary,
        Err(e) => {
            eprintln!("Aviso: sem instância única ({e})");
            Instance::Primary(Server { listener: None })
        }
    }
}

impl Server {
    /// Recebe comandos de outras instâncias numa thread.
    pub fn serve(self, on_cmd: impl Fn(&str) + Send + 'static) {
        let Some(listener) = self.listener else {
            return;
        };
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut s = String::new();
                let _ = stream.read_to_string(&mut s);
                for line in s.lines() {
                    on_cmd(line.trim());
                }
            }
        });
    }
}

pub fn attach_console() {}

// ---------------------------------------------------------------- atualização
/// Espera o processo `pid` terminar (no máximo `timeout`); um zumbi já terminou.
pub fn wait_exit(pid: u32, timeout: Duration) {
    let stat = format!("/proc/{pid}/stat");
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        let running = std::fs::read_to_string(&stat).is_ok_and(|s| {
            // "pid (nome) estado ...": o nome pode ter espaços e parênteses
            s.rsplit_once(')')
                .and_then(|(_, rest)| rest.split_whitespace().next())
                .is_some_and(|state| state != "Z" && state != "X")
        });
        if !running {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Só o instalador do Windows guarda a versão instalada.
pub fn set_installed_version(_dir: &Path, _version: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_never_replaces_pending_apply() {
        let (a, b) = (
            Hotkey::parse("Insert").unwrap(),
            Hotkey::parse("Pause").unwrap(),
        );
        use HotkeyReq::{Apply, Check};
        assert_eq!(merge(Apply(a), Check(a)), Apply(a));
        assert_eq!(merge(Apply(a), Apply(b)), Apply(b));
        assert_eq!(merge(Check(a), Apply(b)), Apply(b));
        assert_eq!(merge(Check(a), Check(b)), Check(b));
    }

    #[test]
    fn uri_decoding() {
        assert_eq!(
            uri_to_path("file:///home/u/Imagens/Captura%20de%20tela%20%C3%A0s.png").unwrap(),
            PathBuf::from("/home/u/Imagens/Captura de tela às.png")
        );
        assert_eq!(
            uri_to_path("file://localhost/tmp/a.png").unwrap(),
            PathBuf::from("/tmp/a.png")
        );
        assert!(uri_to_path("http://x/y").is_none());
        assert_eq!(percent_decode("a%2"), b"a%2");
    }
}
