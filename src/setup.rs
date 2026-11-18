//! `recortei --setup`: configuração por usuário que antes só o install.sh fazia.
//!
//! No Linux: entrada de início automático (`~/.config/autostart`), atalho
//! Insert do GNOME chamando `<este executável> --capture` e permissão do
//! portal de screenshot. Pode rodar quantas vezes quiser (idempotente) e roda
//! sozinho uma vez na primeira abertura (chave `linux_setup_done` do
//! config.json), para quem instala pelo .deb/.rpm/Arch ganhar a tecla Insert
//! sem precisar rodar nada. No Windows não há nada a configurar.

/// Executa `recortei --setup` e devolve o código de saída do processo.
#[cfg(windows)]
pub fn run() -> i32 {
    crate::platform::attach_console();
    println!(
        "No Windows não há nada a configurar: a tecla Insert já é global e o instalador cuida do início automático."
    );
    0
}

/// Na primeira abertura não há o que fazer no Windows.
#[cfg(windows)]
pub fn first_run() {}

#[cfg(target_os = "linux")]
pub use self::linux::{first_run, run};

#[cfg(target_os = "linux")]
mod linux {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use crate::app::APP_ID;
    use crate::config::Config;

    const MEDIA_KEYS: &str = "org.gnome.settings-daemon.plugins.media-keys";
    const KEYBINDING_PATH: &str =
        "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/recortei/";

    /// Executa `recortei --setup` e devolve o código de saída do processo.
    pub fn run() -> i32 {
        let ok = setup();
        if ok {
            let mut cfg = Config::load();
            cfg.set_linux_setup_done();
            println!(">> Pronto.");
            0
        } else {
            1
        }
    }

    /// Roda a configuração uma única vez, na primeira abertura do app. Não
    /// roda a partir da pasta `target/` do cargo, para um `cargo run` não
    /// apontar o atalho e o início automático para o binário de desenvolvimento.
    pub fn first_run() {
        let mut cfg = Config::load();
        if cfg.linux_setup_done() {
            return;
        }
        if current_exe().is_some_and(|exe| inside_cargo_target(&exe)) {
            return;
        }
        println!(">> Primeira execução: configurando o Recortei (recortei --setup)...");
        setup();
        // Mesmo com algum passo falhando, não insiste a cada abertura: o
        // usuário pode rodar `recortei --setup` depois.
        cfg.set_linux_setup_done();
    }

    /// Faz todos os passos; `false` se algum essencial falhou.
    fn setup() -> bool {
        let Some(exe) = current_exe() else {
            eprintln!(">> Erro: não foi possível descobrir o caminho do executável.");
            return false;
        };
        let mut ok = true;
        ok &= autostart(&exe);
        ok &= gnome_shortcut(&exe);
        portal_permission();
        appindicator_hint();
        ok
    }

    /// Caminho do executável atual (sem o " (deleted)" que o Linux acrescenta
    /// quando o binário foi substituído enquanto rodava).
    fn current_exe() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let s = exe.to_str()?;
        Some(PathBuf::from(s.strip_suffix(" (deleted)").unwrap_or(s)))
    }

    /// O binário está dentro de uma pasta `target/` do cargo?
    fn inside_cargo_target(exe: &Path) -> bool {
        exe.ancestors()
            .any(|dir| dir.join("CACHEDIR.TAG").is_file() && dir.join(".rustc_info.json").is_file())
    }

    // ------------------------------------------------------------ início automático
    fn desktop_entry(exe: &Path) -> String {
        format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=Recortei\n\
             Comment=Captura de área de tela com anotações\n\
             Exec={}\n\
             Icon={APP_ID}\n\
             Terminal=false\n\
             Categories=Utility;Graphics;\n\
             StartupNotify=false\n\
             X-GNOME-Autostart-enabled=true\n",
            desktop_exec_arg(&exe.to_string_lossy())
        )
    }

    /// O usuário desativou o início automático nas configurações do sistema?
    fn autostart_disabled(entry: &str) -> bool {
        entry
            .lines()
            .map(|l| l.replace(' ', ""))
            .any(|l| l == "Hidden=true" || l == "X-GNOME-Autostart-enabled=false")
    }

    fn autostart(exe: &Path) -> bool {
        let Some(dir) = dirs::config_dir() else {
            eprintln!(">> Erro: pasta de configuração do usuário não encontrada.");
            return false;
        };
        let file = dir.join("autostart").join(format!("{APP_ID}.desktop"));
        if std::fs::read_to_string(&file).is_ok_and(|s| autostart_disabled(&s)) {
            println!(
                ">> Início automático desativado pelo usuário; mantido como está ({}).",
                file.display()
            );
            return true;
        }
        let result = std::fs::create_dir_all(file.parent().unwrap_or(&dir))
            .and_then(|()| std::fs::write(&file, desktop_entry(exe)));
        match result {
            Ok(()) => {
                println!(">> Início automático: {}", file.display());
                true
            }
            Err(e) => {
                eprintln!(">> Erro gravando {}: {e}", file.display());
                false
            }
        }
    }

    // ------------------------------------------------------------ atalho Insert do GNOME
    fn gsettings(args: &[&str]) -> Option<String> {
        let out = Command::new("gsettings").args(args).output().ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Atalho global Insert: o Wayland não permite o app capturar teclas
    /// sozinho, então registramos um atalho personalizado do GNOME que chama
    /// o app (os mesmos comandos `gsettings` do install.sh).
    fn gnome_shortcut(exe: &Path) -> bool {
        let exe = exe.to_string_lossy();
        let command = format!("{} --capture", shell_quote(&exe));
        let has_schema =
            gsettings(&["list-schemas"]).is_some_and(|s| s.lines().any(|l| l.trim() == MEDIA_KEYS));
        if !has_schema {
            println!(">> GNOME não encontrado: configure no seu ambiente um atalho Insert para: {command}");
            return true;
        }
        let Some(current) = gsettings(&["get", MEDIA_KEYS, "custom-keybindings"]) else {
            eprintln!(">> Erro: não foi possível ler os atalhos personalizados do GNOME.");
            return false;
        };
        let list = add_keybinding(&current, KEYBINDING_PATH);
        let schema = format!("{MEDIA_KEYS}.custom-keybinding:{KEYBINDING_PATH}");
        // Não sobrescreve a tecla que o usuário escolheu nas Configurações:
        // só grava Insert quando o atalho é novo ou está sem tecla.
        let kept = match (&list, gsettings(&["get", &schema, "binding"])) {
            (None, Some(b)) => Some(gvariant_unquote(&b)).filter(|b| !b.is_empty()),
            _ => None,
        };
        // Nome e comando sempre: o caminho do executável pode ter mudado.
        let gsd = gsd_command(&exe);
        let mut keys = vec![("name", "Recortei"), ("command", gsd.as_str())];
        if kept.is_none() {
            keys.push(("binding", "Insert"));
        }
        let mut ok = keys
            .iter()
            .all(|(key, value)| gsettings(&["set", &schema, key, &gvariant_str(value)]).is_some());
        // A lista só ganha o atalho depois que ele está completo.
        if let (true, Some(list)) = (ok, &list) {
            ok = gsettings(&["set", MEDIA_KEYS, "custom-keybindings", list]).is_some();
        }
        match (ok, kept) {
            (true, Some(key)) => {
                println!(">> Atalho do GNOME já existe; tecla mantida ({key}) → {command}")
            }
            (true, None) => println!(">> Atalho do GNOME: Insert → {command}"),
            (false, _) => {
                eprintln!(">> Erro: não foi possível registrar o atalho Insert no GNOME.")
            }
        }
        ok
    }

    /// Comando do atalho como o gnome-settings-daemon espera: ele interpreta
    /// `%` como código de campo do `Exec` (.desktop), então `%` vira `%%`.
    fn gsd_command(exe: &str) -> String {
        format!("{} --capture", shell_quote(exe)).replace('%', "%%")
    }

    /// Acrescenta `path` à lista `custom-keybindings` (texto do `gsettings get`);
    /// `None` se já estiver lá.
    fn add_keybinding(current: &str, path: &str) -> Option<String> {
        let quoted = gvariant_str(path);
        if current.contains(&quoted) {
            return None;
        }
        let current = current.trim();
        let inner = current.strip_prefix("@as").unwrap_or(current).trim();
        let inner = inner
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .unwrap_or("")
            .trim();
        Some(if inner.is_empty() {
            format!("[{quoted}]")
        } else {
            format!("[{inner}, {quoted}]")
        })
    }

    // ------------------------------------------------------------ permissão do portal
    /// Permissão do portal de screenshot para o app (sem isso o GNOME tenta
    /// abrir um diálogo de permissão, que falha porque o app não tem janela
    /// focada). Mesma chamada D-Bus que o install.sh fazia com o `gdbus`.
    fn portal_permission() {
        let result = zbus::blocking::Connection::session().and_then(|c| {
            c.call_method(
                Some("org.freedesktop.impl.portal.PermissionStore"),
                "/org/freedesktop/impl/portal/PermissionStore",
                Some("org.freedesktop.impl.portal.PermissionStore"),
                "SetPermission",
                &("screenshot", true, "screenshot", APP_ID, vec!["yes"]),
            )
        });
        match result {
            Ok(_) => println!(">> Permissão do portal de screenshot concedida."),
            Err(_) => {
                eprintln!(">> Aviso: não foi possível gravar a permissão do portal de screenshot.")
            }
        }
    }

    fn appindicator_hint() {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP")
            .unwrap_or_default()
            .to_lowercase();
        if !desktop.contains("gnome") {
            return;
        }
        let Ok(out) = Command::new("gnome-extensions")
            .args(["list", "--enabled"])
            .output()
        else {
            return;
        };
        if !String::from_utf8_lossy(&out.stdout)
            .to_lowercase()
            .contains("appindicator")
        {
            println!(
                ">> Dica: ative a extensão 'AppIndicator' do GNOME para ver o ícone na bandeja."
            );
        }
    }

    // ------------------------------------------------------------ aspas
    /// String no formato de texto do GVariant (`'...'`), como o `gsettings` espera.
    fn gvariant_str(s: &str) -> String {
        format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
    }

    /// Desfaz [`gvariant_str`] na saída do `gsettings get` (`'Insert'` → `Insert`).
    fn gvariant_unquote(s: &str) -> String {
        let s = s.trim();
        let inner = ['\'', '"']
            .iter()
            .find_map(|&q| s.strip_prefix(q).and_then(|s| s.strip_suffix(q)))
            .unwrap_or(s);
        inner.replace("\\'", "'").replace("\\\\", "\\")
    }

    /// Argumento para um comando que o GNOME divide como o shell.
    fn shell_quote(s: &str) -> String {
        if !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "/._+-=:,@%".contains(c))
        {
            s.to_string()
        } else {
            format!("'{}'", s.replace('\'', "'\\''"))
        }
    }

    /// Argumento da chave `Exec` de um arquivo .desktop (especificação
    /// freedesktop: aspas duplas quando há caracteres reservados, `%` dobrado
    /// e barras invertidas escapadas de novo pela regra de strings).
    fn desktop_exec_arg(s: &str) -> String {
        const RESERVED: &str = " \t\n\"'\\><~|&;$*?#()`";
        let s = s.replace('%', "%%");
        let arg = if s.chars().any(|c| RESERVED.contains(c)) {
            let mut q = String::from("\"");
            for c in s.chars() {
                if "\"`$\\".contains(c) {
                    q.push('\\');
                }
                q.push(c);
            }
            q.push('"');
            q
        } else {
            s
        };
        arg.replace('\\', "\\\\")
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn keybinding_list() {
            let kb = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/recortei/";
            assert_eq!(add_keybinding("@as []", kb).unwrap(), format!("['{kb}']"));
            assert_eq!(add_keybinding("[]", kb).unwrap(), format!("['{kb}']"));
            assert_eq!(
                add_keybinding("['/a/custom0/']", kb).unwrap(),
                format!("['/a/custom0/', '{kb}']")
            );
            assert!(add_keybinding(&format!("['/a/custom0/', '{kb}']"), kb).is_none());
        }

        #[test]
        fn quoting() {
            assert_eq!(gvariant_str("it's"), r"'it\'s'");
            assert_eq!(shell_quote("/usr/bin/recortei"), "/usr/bin/recortei");
            assert_eq!(
                shell_quote("/home/x/Meus Apps/recortei"),
                "'/home/x/Meus Apps/recortei'"
            );
            assert_eq!(shell_quote("/a/it's"), r"'/a/it'\''s'");
            assert_eq!(desktop_exec_arg("/usr/bin/recortei"), "/usr/bin/recortei");
            assert_eq!(
                desktop_exec_arg("/home/x/Meus Apps/recortei"),
                "\"/home/x/Meus Apps/recortei\""
            );
            assert_eq!(desktop_exec_arg("/a/100%/b"), "/a/100%%/b");
            assert_eq!(desktop_exec_arg("/a/$b"), r#""/a/\\$b""#);
            assert_eq!(
                gsd_command("/usr/bin/recortei"),
                "/usr/bin/recortei --capture"
            );
            assert_eq!(gsd_command("/a/100%/rec"), "/a/100%%/rec --capture");
            assert_eq!(
                gsd_command("/home/x/Meus 100%/rec"),
                "'/home/x/Meus 100%%/rec' --capture"
            );
            assert_eq!(gvariant_unquote("'Insert'"), "Insert");
            assert_eq!(gvariant_unquote("'<Super>Print'"), "<Super>Print");
            assert_eq!(gvariant_unquote("''"), "");
            assert_eq!(gvariant_unquote(&gvariant_str("it's")), "it's");
        }

        #[test]
        fn cargo_target_detection() {
            // os testes rodam de target/debug/deps
            assert!(inside_cargo_target(&std::env::current_exe().unwrap()));
            assert!(!inside_cargo_target(Path::new("/usr/bin/recortei")));
        }

        #[test]
        fn autostart_entry() {
            let entry = desktop_entry(Path::new("/usr/bin/recortei"));
            assert!(entry.contains("\nExec=/usr/bin/recortei\n"));
            assert!(entry.contains("\nIcon=io.github.recortei\n"));
            assert!(!autostart_disabled(&entry));
            assert!(autostart_disabled(
                &entry.replace("enabled=true", "enabled=false")
            ));
            assert!(autostart_disabled(&format!("{entry}Hidden=true\n")));
        }
    }
}
