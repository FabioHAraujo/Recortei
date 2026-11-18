//! `recortei --setup` / `--unsetup`: configuração por usuário que antes só o
//! install.sh fazia.
//!
//! No Linux: entrada de início automático (`~/.config/autostart`), atalho de
//! captura do GNOME (padrão PrintScreen) chamando `<este executável> --capture`
//! e permissão do portal de screenshot. Pode rodar quantas vezes quiser
//! (idempotente) e roda sozinho uma vez na primeira abertura (chave
//! `linux_setup_done` do config.json), para quem instala pelo .deb/.rpm/Arch
//! ganhar o atalho sem precisar rodar nada. No Windows não há nada a configurar.

/// Executa `recortei --setup` e devolve o código de saída do processo.
#[cfg(windows)]
pub fn run() -> i32 {
    crate::platform::attach_console();
    let hk = crate::config::Config::load().hotkey();
    println!(
        "No Windows não há nada a configurar: o atalho ({hk}) já é global e o instalador cuida do início automático."
    );
    0
}

/// Executa `recortei --unsetup` e devolve o código de saída do processo.
#[cfg(windows)]
pub fn unsetup() -> i32 {
    crate::platform::attach_console();
    println!("No Windows não há nada a desfazer: o desinstalador remove o início automático.");
    0
}

/// Na primeira abertura não há o que fazer no Windows.
#[cfg(windows)]
pub fn first_run() {}

#[cfg(target_os = "linux")]
pub use self::linux::{first_run, run, unsetup};

#[cfg(target_os = "linux")]
mod linux {
    use std::path::Path;
    use std::process::Command;

    use crate::app::APP_ID;
    use crate::config::Config;
    use crate::gnome;
    use crate::hotkey::Hotkey;

    /// Executa `recortei --setup` e devolve o código de saída do processo.
    pub fn run() -> i32 {
        let ok = setup();
        if ok {
            Config::load().set_linux_setup_done(true);
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
        gnome::adopt_binding(&mut cfg);
        if cfg.linux_setup_done() {
            return;
        }
        if gnome::current_exe().is_some_and(|exe| inside_cargo_target(&exe)) {
            return;
        }
        println!(">> Primeira execução: configurando o Recortei (recortei --setup)...");
        setup();
        // Mesmo com algum passo falhando, não insiste a cada abertura: o
        // usuário pode rodar `recortei --setup` depois.
        cfg.set_linux_setup_done(true);
    }

    /// Executa `recortei --unsetup`: tira o início automático e o atalho do
    /// GNOME e devolve os atalhos de captura do GNOME (para desinstalar).
    pub fn unsetup() -> i32 {
        let mut ok = true;
        if let Some(dir) = dirs::config_dir() {
            let file = dir.join("autostart").join(format!("{APP_ID}.desktop"));
            match std::fs::remove_file(&file) {
                Ok(()) => println!(">> Início automático removido ({}).", file.display()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    eprintln!(">> Erro removendo {}: {e}", file.display());
                    ok = false;
                }
            }
        }
        // só os esquemas: desfaz o que o Recortei gravou mesmo fora do GNOME
        if gnome::installed() {
            match gnome::unbind() {
                Ok(true) => println!(
                    ">> Atalho do GNOME removido; atalhos de captura do GNOME restaurados."
                ),
                Ok(false) => {}
                Err(e) => {
                    eprintln!(">> Erro removendo o atalho do GNOME: {e}");
                    ok = false;
                }
            }
        }
        if ok {
            // a próxima abertura do app configura tudo de novo
            Config::load().set_linux_setup_done(false);
            println!(">> Pronto. Configuração do Recortei desfeita para este usuário.");
            0
        } else {
            1
        }
    }

    /// Faz todos os passos; `false` se algum essencial falhou.
    fn setup() -> bool {
        let Some(exe) = gnome::current_exe() else {
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

    // ------------------------------------------------------------ atalho do GNOME
    /// Atalho global de captura: o Wayland não permite o app capturar teclas
    /// sozinho, então registramos um atalho personalizado do GNOME que chama
    /// o app. Não sobrescreve a tecla que o usuário escolheu nas
    /// Configurações do GNOME (ela é adotada no config.json).
    fn gnome_shortcut(exe: &Path) -> bool {
        let mut cfg = Config::load();
        let command = format!("{} --capture", gnome::shell_quote(&exe.to_string_lossy()));
        if !gnome::available() {
            println!(
                ">> GNOME não encontrado: crie no seu ambiente um atalho global {} para: {command}",
                cfg.hotkey()
            );
            return true;
        }
        gnome::adopt_binding(&mut cfg);
        // tecla que o Recortei não representa (ex.: <Super>space): fica como está
        let kept = gnome::custom_binding().filter(|b| Hotkey::from_gnome(b).is_none());
        let hk = cfg.hotkey();
        match gnome::bind(kept.is_none().then_some(&hk), Some(exe)) {
            Ok(bound) => {
                let key = kept.clone().unwrap_or_else(|| hk.label());
                println!(">> Atalho do GNOME: {key} → {command}");
                for (k, list) in bound.screenshot_changes {
                    println!(
                        ">> Atalho de captura do GNOME '{k}' agora: {list:?} (o original volta se você trocar de tecla ou rodar recortei --unsetup)"
                    );
                }
                if kept.is_none() && !cfg.has_hotkey() {
                    cfg.set_hotkey(hk);
                }
                true
            }
            Err(e) => {
                eprintln!(">> Erro: não foi possível registrar o atalho {hk} no GNOME ({e}).");
                false
            }
        }
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
        fn quoting() {
            assert_eq!(desktop_exec_arg("/usr/bin/recortei"), "/usr/bin/recortei");
            assert_eq!(
                desktop_exec_arg("/home/x/Meus Apps/recortei"),
                "\"/home/x/Meus Apps/recortei\""
            );
            assert_eq!(desktop_exec_arg("/a/100%/b"), "/a/100%%/b");
            assert_eq!(desktop_exec_arg("/a/$b"), r#""/a/\\$b""#);
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
