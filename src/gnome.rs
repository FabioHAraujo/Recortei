//! GNOME: o atalho personalizado do Recortei (gnome-settings-daemon) e os
//! atalhos de captura do próprio GNOME Shell, que usam Print por padrão.
//!
//! Quando o atalho do Recortei coincide com um atalho de captura do GNOME,
//! só aquele acelerador sai da lista do GNOME; a lista original fica guardada
//! no config.json (`gnome_saved_bindings`) e volta quando o usuário escolhe
//! uma tecla que não conflita mais (ou roda `recortei --unsetup`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Config;
use crate::hotkey::Hotkey;

pub const MEDIA_KEYS: &str = "org.gnome.settings-daemon.plugins.media-keys";
pub const KEYBINDING_PATH: &str =
    "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/recortei/";
const SHELL_KEYS: &str = "org.gnome.shell.keybindings";
/// Atalhos de captura do GNOME Shell (padrões: `Print`, `<Shift>Print`, `<Alt>Print`).
const SCREENSHOT_KEYS: [&str; 3] = ["show-screenshot-ui", "screenshot", "screenshot-window"];

type Lists = BTreeMap<String, Vec<String>>;

pub fn gsettings(args: &[&str]) -> Option<String> {
    let out = Command::new("gsettings").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn has_schema(schema: &str) -> bool {
    gsettings(&["list-schemas"]).is_some_and(|s| s.lines().any(|l| l.trim() == schema))
}

/// Os esquemas dos atalhos personalizados do GNOME estão instalados? Eles
/// costumam vir junto em outros ambientes (KDE, Xfce…); para desfazer o que
/// o Recortei gravou basta isso.
pub fn installed() -> bool {
    has_schema(MEDIA_KEYS)
}

/// Esta é uma sessão do GNOME (ou derivado que usa o gnome-settings-daemon),
/// onde o atalho personalizado funciona de verdade?
pub fn available() -> bool {
    gnome_session() && installed()
}

fn gnome_session() -> bool {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    gnome_desktop(&desktop) || media_keys_running()
}

/// `XDG_CURRENT_DESKTOP` (ex.: `ubuntu:GNOME`, `Budgie:GNOME`) é da família do GNOME?
fn gnome_desktop(desktop: &str) -> bool {
    desktop
        .split(':')
        .any(|d| ["gnome", "unity", "budgie"].contains(&d.trim().to_ascii_lowercase().as_str()))
}

/// O gnome-settings-daemon, que atende os atalhos personalizados, está no ar?
fn media_keys_running() -> bool {
    let Ok(c) = zbus::blocking::Connection::session() else {
        return false;
    };
    c.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus"),
        "NameHasOwner",
        &("org.gnome.SettingsDaemon.MediaKeys",),
    )
    .ok()
    .and_then(|r| r.body().deserialize::<bool>().ok())
    .unwrap_or(false)
}

/// Caminho do executável atual (sem o " (deleted)" que o Linux acrescenta
/// quando o binário foi substituído enquanto rodava, como numa atualização
/// pelo .deb/.rpm com o app aberto).
pub fn current_exe() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let s = exe.to_str()?;
    Some(PathBuf::from(strip_deleted(s)))
}

fn strip_deleted(path: &str) -> &str {
    path.strip_suffix(" (deleted)").unwrap_or(path)
}

fn custom_schema() -> String {
    format!("{MEDIA_KEYS}.custom-keybinding:{KEYBINDING_PATH}")
}

/// Grava e confere: sem o D-Bus da sessão (SSH, TTY) o dconf descarta a
/// gravação com só um aviso e o `gsettings` ainda sai com sucesso.
fn set(schema: &str, key: &str, value: &str) -> Result<(), String> {
    gsettings(&["set", schema, key, value])
        .ok_or_else(|| format!("o gsettings recusou a chave {key}"))?;
    match gsettings(&["get", schema, key]) {
        Some(now) if same_value(&now, value) => Ok(()),
        _ => Err(format!(
            "o gsettings não gravou a chave {key} (rode dentro da sessão gráfica)"
        )),
    }
}

/// O valor lido pelo `gsettings get` é o que foi gravado? (listas e strings
/// voltam normalizadas: `@as []`, aspas simples…)
fn same_value(got: &str, wrote: &str) -> bool {
    let wrote = wrote.trim();
    if wrote.starts_with('[') || wrote.starts_with("@as") {
        parse_strv(got) == parse_strv(wrote)
    } else {
        gvariant_unquote(got) == gvariant_unquote(wrote)
    }
}

/// Tecla do atalho do Recortei, se ele está registrado e tem tecla.
pub fn custom_binding() -> Option<String> {
    let list = gsettings(&["get", MEDIA_KEYS, "custom-keybindings"])?;
    if !list.contains(&gvariant_str(KEYBINDING_PATH)) {
        return None;
    }
    let b = gvariant_unquote(&gsettings(&["get", &custom_schema(), "binding"])?);
    (!b.is_empty()).then_some(b)
}

/// O que [`bind`] fez, para o `--setup` contar ao usuário.
pub struct Bound {
    /// Atalhos de captura do GNOME que mudaram: (chave, nova lista).
    pub screenshot_changes: Vec<(String, Vec<String>)>,
}

/// Registra (ou atualiza) o atalho do Recortei com a tecla `hk` (`None`
/// mantém a tecla que já está lá). Com `exe`, grava também nome e comando (o
/// caminho pode ter mudado); sem `exe`, só troca a tecla, a não ser que o
/// atalho ainda não exista.
pub fn bind(hk: Option<&Hotkey>, exe: Option<&Path>) -> Result<Bound, String> {
    let current = gsettings(&["get", MEDIA_KEYS, "custom-keybindings"])
        .ok_or("não foi possível ler os atalhos personalizados do GNOME")?;
    let list = add_keybinding(&current, KEYBINDING_PATH);
    let schema = custom_schema();
    let exe = match exe {
        Some(e) => Some(e.to_path_buf()),
        None if list.is_some() => current_exe(),
        None => None,
    };
    if let Some(exe) = &exe {
        let cmd = gsd_command(&exe.to_string_lossy());
        set(&schema, "name", &gvariant_str("Recortei"))?;
        set(&schema, "command", &gvariant_str(&cmd))?;
    }
    // Primeiro libera a tecla nos atalhos do GNOME, depois grava a nossa: o
    // gnome-settings-daemon só pega a tecla quando o nosso atalho muda.
    let screenshot_changes = reconcile_screenshot_keys(hk)?;
    if let Some(hk) = hk {
        let accel = hk.gnome();
        let before = gsettings(&["get", &schema, "binding"]).map(|b| gvariant_unquote(&b));
        if !screenshot_changes.is_empty() && before.as_deref() == Some(accel.as_str()) {
            set(&schema, "binding", "''")?; // força o daemon a pegar a tecla de novo
        }
        set(&schema, "binding", &gvariant_str(&accel))?;
    }
    // A lista só ganha o atalho depois que ele está completo.
    if let Some(list) = list {
        set(MEDIA_KEYS, "custom-keybindings", &list)?;
    }
    Ok(Bound { screenshot_changes })
}

/// Atualização sem surpresa: se o atalho do Recortei no GNOME tem uma tecla
/// (escolhida pelo usuário nas Configurações do GNOME ou o Insert das versões
/// antigas) diferente da do config.json, ela passa a valer. Os atalhos de
/// captura do GNOME acompanham: a tecla que o Recortei não usa mais volta
/// para o GNOME (sem atalho do Recortei, volta tudo).
pub fn adopt_binding(cfg: &mut Config) {
    match custom_binding().and_then(|b| Hotkey::from_gnome(&b)) {
        Some(hk) => {
            if !cfg.has_hotkey() || cfg.hotkey() != hk {
                cfg.set_hotkey(hk);
            }
            let _ = reconcile_screenshot_keys(Some(&hk));
        }
        // sem tecla ou com uma que o Recortei não representa (ex.: <Super>space)
        None if !cfg.gnome_saved_bindings().is_empty() => {
            let _ = reconcile_screenshot_keys(None);
        }
        None => {}
    }
}

/// Tira o atalho do Recortei e devolve os atalhos de captura do GNOME.
/// Devolve se havia algo do Recortei para remover.
pub fn unbind() -> Result<bool, String> {
    let current = gsettings(&["get", MEDIA_KEYS, "custom-keybindings"])
        .ok_or("não foi possível ler os atalhos personalizados do GNOME")?;
    let mut removed = false;
    if let Some(list) = remove_keybinding(&current, KEYBINDING_PATH) {
        set(MEDIA_KEYS, "custom-keybindings", &list)?;
        removed = true;
    }
    let schema = custom_schema();
    for key in ["name", "command", "binding"] {
        let _ = gsettings(&["reset", &schema, key]);
    }
    let restored = reconcile_screenshot_keys(None)?;
    Ok(removed || !restored.is_empty())
}

/// Ajusta os atalhos de captura do GNOME para não disputarem `target` com o
/// Recortei (`None` devolve tudo). Idempotente. Devolve o que mudou.
pub fn reconcile_screenshot_keys(
    target: Option<&Hotkey>,
) -> Result<Vec<(String, Vec<String>)>, String> {
    if !has_schema(SHELL_KEYS) {
        return Ok(Vec::new());
    }
    let mut current = Lists::new();
    for key in SCREENSHOT_KEYS {
        if let Some(v) = gsettings(&["get", SHELL_KEYS, key]) {
            current.insert(key.into(), parse_strv(&v));
        }
    }
    let mut cfg = Config::load();
    let plan = plan_screenshot_keys(&current, &cfg.gnome_saved_bindings(), target);
    let mut changes = Vec::new();
    let mut saved = Lists::new();
    let mut error = None;
    for p in plan {
        let mut now = p.current;
        if p.want != now {
            match set(SHELL_KEYS, &p.key, &format_strv(&p.want)) {
                Ok(()) => {
                    now = p.want.clone();
                    changes.push((p.key.clone(), p.want));
                }
                Err(e) => error = Some(e),
            }
        }
        if now != p.original {
            saved.insert(p.key, p.original);
        }
    }
    cfg.set_gnome_saved_bindings(&saved);
    match error {
        Some(e) => Err(e),
        None => Ok(changes),
    }
}

#[derive(Debug, PartialEq)]
struct KeyPlan {
    key: String,
    current: Vec<String>,
    /// Lista do GNOME antes do Recortei mexer.
    original: Vec<String>,
    want: Vec<String>,
}

/// Decide o valor de cada atalho de captura do GNOME. A lista guardada só
/// vale enquanto a atual for ela menos alguns itens; se o usuário mudou o
/// atalho por conta própria depois, a dele passa a ser a original.
fn plan_screenshot_keys(current: &Lists, saved: &Lists, target: Option<&Hotkey>) -> Vec<KeyPlan> {
    current
        .iter()
        .map(|(key, cur)| {
            let original = match saved.get(key) {
                Some(o) if cur.iter().all(|a| o.contains(a)) => o.clone(),
                _ => cur.clone(),
            };
            let want = original
                .iter()
                .filter(|a| !target.is_some_and(|t| same_accel(a, t)))
                .cloned()
                .collect();
            KeyPlan {
                key: key.clone(),
                current: cur.clone(),
                original,
                want,
            }
        })
        .collect()
}

/// `accel` (como o GNOME guarda: `<Ctrl>Print`, `<Primary>Print`…) é a mesma tecla de `hk`?
fn same_accel(accel: &str, hk: &Hotkey) -> bool {
    Hotkey::from_gnome(accel).map_or(accel == hk.gnome(), |a| a == *hk)
}

// ------------------------------------------------------------ listas do gsettings
/// Lê uma lista de strings no formato de texto do GVariant (`['a', 'b']`, `@as []`).
fn parse_strv(s: &str) -> Vec<String> {
    let s = s.trim();
    let s = s.strip_prefix("@as").unwrap_or(s).trim();
    let Some(inner) = s.strip_prefix('[').and_then(|s| s.strip_suffix(']')) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\'' && c != '"' {
            continue; // vírgulas e espaços entre os itens
        }
        let mut item = String::new();
        while let Some(d) = chars.next() {
            match d {
                '\\' => item.extend(chars.next()),
                _ if d == c => break,
                _ => item.push(d),
            }
        }
        out.push(item);
    }
    out
}

fn format_strv(items: &[String]) -> String {
    if items.is_empty() {
        return "@as []".into();
    }
    let parts: Vec<String> = items.iter().map(|s| gvariant_str(s)).collect();
    format!("[{}]", parts.join(", "))
}

/// Acrescenta `path` à lista `custom-keybindings`; `None` se já estiver lá.
fn add_keybinding(current: &str, path: &str) -> Option<String> {
    let mut list = parse_strv(current);
    if list.iter().any(|p| p == path) {
        return None;
    }
    list.push(path.into());
    Some(format_strv(&list))
}

/// Tira `path` da lista `custom-keybindings`; `None` se não estiver lá.
fn remove_keybinding(current: &str, path: &str) -> Option<String> {
    let list = parse_strv(current);
    let kept: Vec<String> = list.iter().filter(|p| *p != path).cloned().collect();
    (kept.len() != list.len()).then(|| format_strv(&kept))
}

// ------------------------------------------------------------ aspas
/// String no formato de texto do GVariant (`'...'`), como o `gsettings` espera.
pub fn gvariant_str(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// Desfaz [`gvariant_str`] na saída do `gsettings get` (`'Insert'` → `Insert`).
pub fn gvariant_unquote(s: &str) -> String {
    let s = s.trim();
    let inner = ['\'', '"']
        .iter()
        .find_map(|&q| s.strip_prefix(q).and_then(|s| s.strip_suffix(q)))
        .unwrap_or(s);
    inner.replace("\\'", "'").replace("\\\\", "\\")
}

/// Argumento para um comando que o GNOME divide como o shell.
pub fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._+-=:,@%".contains(c))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Comando do atalho como o gnome-settings-daemon espera: ele interpreta
/// `%` como código de campo do `Exec` (.desktop), então `%` vira `%%`.
fn gsd_command(exe: &str) -> String {
    format!("{} --capture", shell_quote(exe)).replace('%', "%%")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn defaults() -> Lists {
        Lists::from([
            ("screenshot".into(), v(&["<Shift>Print"])),
            ("screenshot-window".into(), v(&["<Alt>Print"])),
            ("show-screenshot-ui".into(), v(&["Print"])),
        ])
    }

    /// Aplica o plano como o `reconcile_screenshot_keys` faz (sem gsettings).
    fn run(current: &mut Lists, saved: &mut Lists, target: Option<&str>) {
        let hk = target.map(|t| Hotkey::parse(t).unwrap());
        let plan = plan_screenshot_keys(current, saved, hk.as_ref());
        saved.clear();
        for p in plan {
            if p.want != p.original {
                saved.insert(p.key.clone(), p.original.clone());
            }
            current.insert(p.key, p.want);
        }
    }

    #[test]
    fn screenshot_keys_lifecycle() {
        let (mut cur, mut saved) = (defaults(), Lists::new());
        // Insert não conflita: nada muda, nada é guardado
        run(&mut cur, &mut saved, Some("Insert"));
        assert_eq!((&cur, saved.is_empty()), (&defaults(), true));
        // PrintScreen: só o Print sai do show-screenshot-ui
        run(&mut cur, &mut saved, Some("PrintScreen"));
        assert_eq!(cur["show-screenshot-ui"], Vec::<String>::new());
        assert_eq!(cur["screenshot"], v(&["<Shift>Print"]));
        assert_eq!(
            saved,
            Lists::from([("show-screenshot-ui".into(), v(&["Print"]))])
        );
        // idempotente
        let before = (cur.clone(), saved.clone());
        run(&mut cur, &mut saved, Some("PrintScreen"));
        assert_eq!((cur.clone(), saved.clone()), before);
        // troca para Alt+PrintScreen: Print volta, <Alt>Print sai
        run(&mut cur, &mut saved, Some("Alt+PrintScreen"));
        assert_eq!(cur["show-screenshot-ui"], v(&["Print"]));
        assert_eq!(cur["screenshot-window"], Vec::<String>::new());
        assert_eq!(saved.keys().collect::<Vec<_>>(), ["screenshot-window"]);
        // tecla sem conflito: tudo restaurado e nada guardado
        run(&mut cur, &mut saved, Some("Ctrl+PrintScreen"));
        assert_eq!((&cur, saved.is_empty()), (&defaults(), true));
        // --unsetup depois de PrintScreen
        run(&mut cur, &mut saved, Some("PrintScreen"));
        run(&mut cur, &mut saved, None);
        assert_eq!((&cur, saved.is_empty()), (&defaults(), true));
    }

    #[test]
    fn screenshot_keys_respect_user_changes() {
        let mut cur = defaults();
        cur.insert("show-screenshot-ui".into(), v(&["Print", "<Super>p"]));
        let mut saved = Lists::new();
        run(&mut cur, &mut saved, Some("PrintScreen"));
        assert_eq!(
            cur["show-screenshot-ui"],
            v(&["<Super>p"]),
            "só o Print sai"
        );
        // o usuário trocou o atalho do GNOME por conta própria: a escolha dele vale
        cur.insert("show-screenshot-ui".into(), v(&["<Super>x"]));
        run(&mut cur, &mut saved, Some("Insert"));
        assert_eq!(cur["show-screenshot-ui"], v(&["<Super>x"]));
        assert!(saved.is_empty());
        // aceleradores escritos de outro jeito (<Primary>, <Ctrl>) também contam
        let mut cur = Lists::from([("screenshot".into(), v(&["<Primary>Print", "<Shift>Print"]))]);
        let mut saved = Lists::new();
        run(&mut cur, &mut saved, Some("Ctrl+PrintScreen"));
        assert_eq!(cur["screenshot"], v(&["<Shift>Print"]));
    }

    /// Ponta a ponta com o gsettings de verdade. Mexe nos atalhos do GNOME do
    /// usuário, então só roda com `RECORTEI_GNOME_E2E=1`, num contêiner
    /// descartável (HOME temporário, dconf próprio):
    /// `RECORTEI_GNOME_E2E=1 cargo test gnome_end_to_end -- --ignored`.
    #[test]
    #[ignore]
    fn gnome_end_to_end() {
        if std::env::var("RECORTEI_GNOME_E2E").as_deref() != Ok("1") {
            eprintln!("pulado: defina RECORTEI_GNOME_E2E=1 (só em contêiner descartável)");
            return;
        }
        let shell = |k: &str| parse_strv(&gsettings(&["get", SHELL_KEYS, k]).unwrap());
        let list = || gsettings(&["get", MEDIA_KEYS, "custom-keybindings"]).unwrap();
        let hk = |s: &str| Hotkey::parse(s).unwrap();
        assert!(
            available() && has_schema(SHELL_KEYS),
            "esquemas do GNOME instalados"
        );
        let defaults = |what: &str| {
            assert_eq!(shell("show-screenshot-ui"), v(&["Print"]), "{what}");
            assert_eq!(shell("screenshot"), v(&["<Shift>Print"]), "{what}");
            assert_eq!(shell("screenshot-window"), v(&["<Alt>Print"]), "{what}");
            assert!(Config::load().gnome_saved_bindings().is_empty(), "{what}");
        };
        defaults("início");

        let exe = Path::new("/opt/Meus Apps/recortei");
        let b = bind(Some(&hk("PrintScreen")), Some(exe)).unwrap();
        assert_eq!(
            b.screenshot_changes,
            vec![("show-screenshot-ui".into(), v(&[]))]
        );
        assert_eq!(custom_binding().as_deref(), Some("Print"));
        assert!(list().contains(KEYBINDING_PATH));
        let cmd = gsettings(&["get", &custom_schema(), "command"]).unwrap();
        assert_eq!(
            gvariant_unquote(&cmd),
            "'/opt/Meus Apps/recortei' --capture"
        );
        assert_eq!(shell("show-screenshot-ui"), v(&[]));
        assert_eq!(shell("screenshot"), v(&["<Shift>Print"]));
        assert_eq!(
            Config::load().gnome_saved_bindings(),
            Lists::from([("show-screenshot-ui".into(), v(&["Print"]))])
        );

        // idempotente
        let b = bind(Some(&hk("PrintScreen")), Some(exe)).unwrap();
        assert!(b.screenshot_changes.is_empty());
        assert_eq!(list().matches(KEYBINDING_PATH).count(), 1);

        // troca para Alt+PrintScreen (pela janela: sem exe) e depois sem conflito
        bind(Some(&hk("Alt+PrintScreen")), None).unwrap();
        assert_eq!(custom_binding().as_deref(), Some("<Alt>Print"));
        assert_eq!(shell("show-screenshot-ui"), v(&["Print"]));
        assert_eq!(shell("screenshot-window"), v(&[]));
        bind(Some(&hk("Ctrl+Shift+S")), None).unwrap();
        assert_eq!(custom_binding().as_deref(), Some("<Control><Shift>s"));
        defaults("depois de Ctrl+Shift+S");
        assert_eq!(
            gvariant_unquote(&gsettings(&["get", &custom_schema(), "command"]).unwrap()),
            "'/opt/Meus Apps/recortei' --capture",
            "trocar a tecla não mexe no comando"
        );

        // o usuário troca a tecla nas Configurações do GNOME e abre o app:
        // a tecla é adotada e o Print volta para o GNOME
        bind(Some(&hk("PrintScreen")), None).unwrap();
        assert_eq!(shell("show-screenshot-ui"), v(&[]));
        gsettings(&["set", &custom_schema(), "binding", "'Insert'"]).unwrap();
        let mut cfg = Config::load();
        adopt_binding(&mut cfg);
        assert_eq!(Config::load().hotkey(), hk("Insert"));
        defaults("depois de adotar Insert");
        // ... ou apaga a tecla do Recortei: tudo volta para o GNOME
        bind(Some(&hk("PrintScreen")), None).unwrap();
        gsettings(&["set", &custom_schema(), "binding", "''"]).unwrap();
        adopt_binding(&mut Config::load());
        defaults("depois de apagar a tecla do Recortei");

        // --unsetup devolve tudo
        bind(Some(&hk("PrintScreen")), None).unwrap();
        assert!(unbind().unwrap());
        defaults("depois do unbind");
        assert!(!list().contains(KEYBINDING_PATH));
        assert_eq!(custom_binding(), None);
        assert!(!unbind().unwrap(), "segunda vez não há nada a remover");
    }

    #[test]
    fn gnome_family() {
        for d in [
            "GNOME",
            "ubuntu:GNOME",
            "Budgie:GNOME",
            "Unity",
            "gnome-classic:GNOME",
        ] {
            assert!(gnome_desktop(d), "{d}");
        }
        for d in ["", "KDE", "XFCE", "X-Cinnamon", "LXQt", "sway"] {
            assert!(!gnome_desktop(d), "{d}");
        }
    }

    #[test]
    fn exe_after_upgrade() {
        assert_eq!(
            strip_deleted("/usr/bin/recortei (deleted)"),
            "/usr/bin/recortei"
        );
        assert_eq!(strip_deleted("/usr/bin/recortei"), "/usr/bin/recortei");
    }

    #[test]
    fn written_values_compare_normalised() {
        assert!(same_value("@as []", "@as []"));
        assert!(same_value("@as []\n", "[]"));
        assert!(same_value("['Print']", "['Print']"));
        assert!(same_value(
            "['<Shift>Print', '<Super>s']",
            "[\"<Shift>Print\", '<Super>s']"
        ));
        assert!(
            !same_value("['Print']", "@as []"),
            "gravação perdida (sem D-Bus)"
        );
        assert!(same_value("'Print'", "'Print'"));
        assert!(same_value("''", "''"));
        assert!(!same_value("'Print'", "'Insert'"));
        // o GLib imprime com aspas duplas strings que têm aspas simples
        assert!(same_value(
            "\"'/opt/Meus Apps/recortei' --capture\"",
            &gvariant_str("'/opt/Meus Apps/recortei' --capture")
        ));
    }

    #[test]
    fn strv_round_trip() {
        assert_eq!(parse_strv("@as []"), Vec::<String>::new());
        assert_eq!(parse_strv("[]"), Vec::<String>::new());
        assert_eq!(parse_strv("['Print']"), v(&["Print"]));
        assert_eq!(
            parse_strv("['<Shift>Print', \"<Super>s\"]"),
            v(&["<Shift>Print", "<Super>s"])
        );
        assert_eq!(parse_strv(r"['it\'s', 'a\\b']"), v(&["it's", r"a\b"]));
        assert_eq!(format_strv(&[]), "@as []");
        assert_eq!(format_strv(&v(&["Print", "it's"])), r"['Print', 'it\'s']");
        for s in [v(&[]), v(&["a"]), v(&["<Alt>Print", "x'y", r"z\w"])] {
            assert_eq!(parse_strv(&format_strv(&s)), s);
        }
    }

    #[test]
    fn keybinding_list() {
        let kb = KEYBINDING_PATH;
        assert_eq!(add_keybinding("@as []", kb).unwrap(), format!("['{kb}']"));
        assert_eq!(add_keybinding("[]", kb).unwrap(), format!("['{kb}']"));
        assert_eq!(
            add_keybinding("['/a/custom0/']", kb).unwrap(),
            format!("['/a/custom0/', '{kb}']")
        );
        assert!(add_keybinding(&format!("['/a/custom0/', '{kb}']"), kb).is_none());
        assert_eq!(
            remove_keybinding(&format!("['/a/custom0/', '{kb}']"), kb).unwrap(),
            "['/a/custom0/']"
        );
        assert_eq!(
            remove_keybinding(&format!("['{kb}']"), kb).unwrap(),
            "@as []"
        );
        assert!(remove_keybinding("['/a/custom0/']", kb).is_none());
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
}
