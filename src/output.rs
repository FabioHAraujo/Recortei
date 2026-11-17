//! Destino do recorte: área de transferência, arquivo PNG e notificação.

use std::borrow::Cow;
use std::path::PathBuf;

use tiny_skia::Pixmap;

use crate::config::save_dir;

/// Caminho livre em `dir` para `Recorte_<data>.png` (acrescenta _2, _3… se já existir).
pub fn unique_path(dir: &std::path::Path, stamp: &str) -> PathBuf {
    let mut path = dir.join(format!("Recorte_{stamp}.png"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("Recorte_{stamp}_{n}.png"));
        n += 1;
    }
    path
}

/// Salva em `~/Imagens/Recortei/Recorte_AAAA-MM-DD_HH-MM-SS.png`.
pub fn save_png(pm: &Pixmap) -> Result<PathBuf, String> {
    let dir = save_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Erro criando {}: {e}", dir.display()))?;
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let path = unique_path(&dir, &stamp);
    pm.save_png(&path).map_err(|e| format!("Erro salvando {}: {e}", path.display()))?;
    Ok(path)
}

/// Mantém o objeto do clipboard vivo: no X11 é ele quem entrega os dados
/// para quem colar, então não pode ser destruído logo após copiar.
#[derive(Default)]
pub struct Clipboard {
    inner: Option<arboard::Clipboard>,
}

impl Clipboard {
    pub fn set_image(&mut self, pm: &Pixmap) -> Result<(), String> {
        let image = arboard::ImageData {
            width: pm.width() as usize,
            height: pm.height() as usize,
            // a imagem é opaca, então pré-multiplicado == RGBA comum
            bytes: Cow::Borrowed(pm.data()),
        };
        let first = (|| -> Result<(), arboard::Error> {
            if self.inner.is_none() {
                self.inner = Some(arboard::Clipboard::new()?);
            }
            self.inner.as_mut().expect("criado acima").set_image(image)
        })();
        match first {
            Ok(()) => Ok(()),
            Err(e) => {
                self.inner = None;
                #[cfg(target_os = "linux")]
                if linux_fallback(pm).is_ok() {
                    return Ok(());
                }
                Err(e.to_string())
            }
        }
    }
}

/// Se o arboard falhar, tenta `wl-copy` (Wayland) ou `xclip` (X11).
#[cfg(target_os = "linux")]
fn linux_fallback(pm: &Pixmap) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let png = pm.encode_png().map_err(|e| e.to_string())?;
    let cmds: [(&str, &[&str]); 2] =
        [("wl-copy", &["--type", "image/png"]), ("xclip", &["-selection", "clipboard", "-t", "image/png"])];
    for (cmd, args) in cmds {
        let Ok(mut child) = Command::new(cmd).args(args).stdin(Stdio::piped()).spawn() else { continue };
        if let Some(mut stdin) = child.stdin.take() {
            if stdin.write_all(&png).is_ok() {
                drop(stdin);
                // wl-copy/xclip ficam em segundo plano servindo os dados
                std::thread::spawn(move || child.wait());
                return Ok(());
            }
        }
    }
    Err("nenhum utilitário de clipboard encontrado".into())
}

/// Notificação do sistema, enviada numa thread para não travar a interface.
pub fn notify(title: &str, body: &str) {
    let (title, body) = (title.to_owned(), body.to_owned());
    std::thread::spawn(move || {
        if let Err(e) = show_notification(&title, &body) {
            eprintln!("{title}: {body} (notificação falhou: {e})");
        }
    });
}

#[cfg(windows)]
fn show_notification(title: &str, body: &str) -> Result<(), String> {
    notify_rust::Notification::new()
        .summary(title)
        .body(body)
        .appname("Recortei")
        .show()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// `org.freedesktop.Notifications.Notify` direto pelo D-Bus.
#[cfg(target_os = "linux")]
fn show_notification(title: &str, body: &str) -> Result<(), String> {
    use std::collections::HashMap;
    use zbus::zvariant::Value;
    let conn = zbus::blocking::Connection::session().map_err(|e| e.to_string())?;
    let mut hints: HashMap<&str, Value> = HashMap::new();
    hints.insert("desktop-entry", Value::from(crate::app::APP_ID));
    conn.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &("Recortei", 0u32, "applets-screenshooter", title, body, Vec::<&str>::new(), hints, 5000i32),
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_path_avoids_overwriting() {
        let dir = std::env::temp_dir().join(format!("recortei-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = unique_path(&dir, "x");
        assert!(a.ends_with("Recorte_x.png"));
        std::fs::write(&a, b"").unwrap();
        let b = unique_path(&dir, "x");
        assert!(b.ends_with("Recorte_x_2.png"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
