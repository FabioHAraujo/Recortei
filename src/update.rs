//! Atualização automática, sem passar pelo instalador. É o mesmo modelo do
//! updater do Tauri usado no Zynko e nos apps da LowBloat:
//!
//! - pouco depois de abrir e depois de hora em hora, baixa o `latest.json` da
//!   última release no GitHub;
//! - havendo versão nova, baixa o executável da plataforma e confere a
//!   assinatura minisign (feita com `tauri signer sign`) com a chave pública
//!   embutida (`packaging/updater.pub`), inclusive se ela é daquele arquivo,
//!   com aquela versão no nome;
//! - roda `<novo> --version` para conferir que ele abre nesta máquina e troca
//!   o executável no lugar (`self-replace`). A versão nova vale no próximo
//!   início, e o menu da bandeja oferece "Reiniciar para atualizar";
//! - instalado por um gerenciador de pacotes (`/usr`, `/opt`…) ou numa pasta
//!   sem permissão de escrita, só avisa que há versão nova.
//!
//! Fica desligado nas builds de debug e com `RECORTEI_NO_UPDATE=1`. Para
//! testar com outro servidor, `RECORTEI_UPDATE_URL` troca o endereço do
//! `latest.json` (a assinatura continua obrigatória).

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use base64::Engine;
use serde::Deserialize;
use winit::event_loop::EventLoopProxy;

use crate::app::UserEvent;
use crate::platform;

pub const MANIFEST_URL: &str =
    "https://github.com/FabioHAraujo/Recortei/releases/latest/download/latest.json";
pub const RELEASES_URL: &str = "https://github.com/FabioHAraujo/Recortei/releases";
/// Chave pública do updater, no formato do `tauri signer` (base64 do `.pub`
/// do minisign). A privada fica em `~/.tauri/recortei-updater.key` e nos
/// secrets do repositório.
const PUBLIC_KEY: &str = include_str!("../packaging/updater.pub");
const CURRENT: &str = env!("CARGO_PKG_VERSION");
const FIRST_CHECK: Duration = Duration::from_secs(30);
const EVERY: Duration = Duration::from_secs(60 * 60);
const MAX_MANIFEST: u64 = 256 * 1024;
const MAX_BINARY: u64 = 64 * 1024 * 1024;

#[derive(Deserialize)]
struct Manifest {
    version: String,
    #[serde(default)]
    platforms: HashMap<String, Asset>,
}

#[derive(Deserialize)]
struct Asset {
    url: String,
    signature: String,
}

/// O que dá para fazer com uma versão nova nesta instalação.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Mode {
    /// Troca o executável que está neste caminho.
    Replace(PathBuf),
    /// Só avisa (pacote do sistema ou pasta sem escrita).
    Notify,
}

/// Resultado de uma verificação com versão nova.
enum Found {
    /// Já trocada no disco: vale ao reiniciar o executável deste caminho.
    Ready(String, PathBuf),
    /// Existe, mas a instalação não se atualiza sozinha.
    Available(String),
}

/// Chave da plataforma no `latest.json` (a mesma do Tauri: `windows-x86_64`, `linux-x86_64`).
pub fn platform_key() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Nome do executável de uma versão na release; a assinatura vale para este nome.
pub fn asset_name(version: &str, platform: &str) -> String {
    let ext = if platform.starts_with("windows") {
        ".exe"
    } else {
        ""
    };
    format!("recortei-{version}-{platform}{ext}")
}

/// Começa a verificar numa thread; o que achar chega como `UserEvent`.
pub fn start(proxy: EventLoopProxy<UserEvent>) {
    let off = std::env::var_os("RECORTEI_NO_UPDATE").is_some_and(|v| v != "0");
    if cfg!(debug_assertions) || off {
        return;
    }
    let Some(mode) = mode() else { return };
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        let mut announced: Option<String> = None;
        loop {
            match check(&mode, announced.as_deref()) {
                Ok(Some(Found::Ready(version, exe))) => {
                    let _ = proxy.send_event(UserEvent::UpdateReady(version, exe));
                    return; // o executável já é o novo; continua depois de reiniciar
                }
                Ok(Some(Found::Available(version))) => {
                    announced = Some(version.clone());
                    let _ = proxy.send_event(UserEvent::UpdateAvailable(version));
                }
                Ok(None) => {}
                Err(e) => eprintln!("Atualização: {e}"),
            }
            std::thread::sleep(EVERY);
        }
    });
}

fn mode() -> Option<Mode> {
    let exe = std::env::current_exe().ok()?;
    #[cfg(target_os = "linux")]
    let exe = std::fs::canonicalize(&exe).ok()?;
    let system = ["/usr", "/opt", "/nix", "/snap", "/app"];
    if cfg!(target_os = "linux") && system.iter().any(|p| exe.starts_with(p)) {
        return Some(Mode::Notify);
    }
    if !writable(exe.parent()?) {
        return Some(Mode::Notify);
    }
    Some(Mode::Replace(exe))
}

fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".recortei-probe-{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(300)))
        .timeout_connect(Some(Duration::from_secs(20)))
        .user_agent(concat!("Recortei/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

fn download(agent: &ureq::Agent, url: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut resp = agent
        .get(url)
        .call()
        .map_err(|e| format!("falha ao baixar {url}: {e}"))?;
    resp.body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .map_err(|e| format!("falha ao baixar {url}: {e}"))
}

fn check(mode: &Mode, announced: Option<&str>) -> Result<Option<Found>, String> {
    let agent = agent();
    let url = std::env::var("RECORTEI_UPDATE_URL").unwrap_or_else(|_| MANIFEST_URL.into());
    let manifest = download(&agent, &url, MAX_MANIFEST)?;
    let manifest: Manifest =
        serde_json::from_slice(&manifest).map_err(|e| format!("latest.json inválido: {e}"))?;
    let version = manifest.version.trim_start_matches('v').to_owned();
    if !is_newer(&version, CURRENT) || announced == Some(version.as_str()) {
        return Ok(None);
    }
    let platform = platform_key();
    let (Mode::Replace(exe), Some(asset)) = (mode, manifest.platforms.get(&platform)) else {
        return Ok(Some(Found::Available(version)));
    };
    let bin = download(&agent, &asset.url, MAX_BINARY)?;
    verify(
        PUBLIC_KEY,
        &bin,
        &asset.signature,
        &asset_name(&version, &platform),
    )?;
    install(exe, &bin, &version)?;
    Ok(Some(Found::Ready(version, exe.clone())))
}

/// `a` é mais nova que `b`? Compara `x.y.z` numericamente; sufixos (`-rc1`)
/// contam como anteriores à versão sem sufixo.
pub fn is_newer(a: &str, b: &str) -> bool {
    fn parse(v: &str) -> Option<(Vec<u64>, bool)> {
        let v = v.trim().trim_start_matches('v');
        let (core, pre) = match v.split_once('-') {
            Some((c, _)) => (c, true),
            None => (v, false),
        };
        let mut nums = core
            .split('.')
            .map(|p| p.parse().ok())
            .collect::<Option<Vec<u64>>>()?;
        nums.resize(3, 0);
        Some((nums, pre))
    }
    match (parse(a), parse(b)) {
        (Some((na, pa)), Some((nb, pb))) => na > nb || (na == nb && pb && !pa),
        _ => false,
    }
}

/// Confere a assinatura (`signature`: base64 do `.sig` do `tauri signer`) e
/// se ela foi feita para o arquivo `file` (o nome leva a versão).
pub fn verify(public_key: &str, bin: &[u8], signature: &str, file: &str) -> Result<(), String> {
    let b64 = |s: &str, what: &str| -> Result<String, String> {
        let raw = base64::engine::general_purpose::STANDARD
            .decode(s.trim())
            .map_err(|e| format!("{what} inválida: {e}"))?;
        String::from_utf8(raw).map_err(|e| format!("{what} inválida: {e}"))
    };
    let pk = minisign_verify::PublicKey::decode(&b64(public_key, "chave pública")?)
        .map_err(|e| format!("chave pública inválida: {e}"))?;
    let sig = minisign_verify::Signature::decode(&b64(signature, "assinatura")?)
        .map_err(|e| format!("assinatura inválida: {e}"))?;
    pk.verify(bin, &sig, false)
        .map_err(|e| format!("assinatura não confere: {e}"))?;
    let expected = format!("file:{file}");
    if !sig.trusted_comment().split('\t').any(|f| f == expected) {
        return Err(format!(
            "a assinatura é de outro arquivo ({}), não de {file}",
            sig.trusted_comment()
        ));
    }
    Ok(())
}

/// Grava o executável novo ao lado do atual, confere que ele abre e diz a
/// versão certa, e troca o atual por ele.
fn install(exe: &Path, bin: &[u8], version: &str) -> Result<(), String> {
    let dir = exe.parent().ok_or("executável sem pasta")?;
    let tmp = dir.join(if cfg!(windows) {
        ".recortei-update.exe"
    } else {
        ".recortei-update"
    });
    let result = (|| {
        std::fs::write(&tmp, bin).map_err(|e| format!("falha ao gravar {}: {e}", tmp.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| e.to_string())?;
        }
        check_runs(&tmp, version)?;
        self_replace::self_replace(&tmp).map_err(|e| format!("falha ao trocar o executável: {e}"))
    })();
    let _ = std::fs::remove_file(&tmp);
    result?;
    platform::set_installed_version(dir, version);
    Ok(())
}

/// `<novo> --version` tem de responder `recortei <versão>` em pouco tempo.
fn check_runs(exe: &Path, version: &str) -> Result<(), String> {
    let mut cmd = Command::new(exe);
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("a versão nova não abre: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("a versão nova não respondeu ao --version".into());
            }
        }
    }
    let mut out = String::new();
    if let Some(mut s) = child.stdout.take() {
        let _ = s.read_to_string(&mut out);
    }
    let expected = format!("recortei {version}");
    if out.trim() != expected {
        return Err(format!(
            "a versão nova respondeu {:?} em vez de {expected:?}",
            out.trim()
        ));
    }
    Ok(())
}

/// Abre o executável (já atualizado) de novo e pede para ele esperar esta
/// instância fechar antes de assumir a bandeja.
pub fn relaunch(exe: &Path) -> Result<(), String> {
    Command::new(exe)
        .arg("--after-update")
        .arg(std::process::id().to_string())
        .stdin(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Chave e assinatura descartáveis, geradas só para estes testes com
    // `tauri signer generate` / `tauri signer sign` (2.12.6).
    const TEST_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDk5RjlDNTE0NUZENzZDQ0EKUldUS2JOZGZGTVg1bVlGcVVpaXEvUnovTFdtOFVCMzBycXZqTHRmV2xib1VmUlo1czF1MFZwdW4K";
    const TEST_SIG: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUS2JOZGZGTVg1bWFOdjcwaU4wekRvQmt2VUdBMDI5QnhNZkh6VU8xeDZTUU9teWNqeUtFdEhaWjF3MGx0VnlOZEtnNFFyYVoyek5xbkpXanJxb081cXlIUmhBU3ExZVFjPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk1MDU2MjU4CWZpbGU6cmVjb3J0ZWktOS45Ljktd2luZG93cy14ODZfNjQuZXhlCjFGRXhPYVdZcnJKd2dYa004UzdLS1oySmh1bWtjSkNMQnE1NG53NGY0M2VaSllhaGxPc01GUHRLME9TNzNUVFBlZ0dWN3A3MUV5YXNueCtsLzFPMERRPT0K";
    const TEST_BIN: &[u8] = b"binario de teste do Recortei\n";

    #[test]
    fn signature_round_trip() {
        let file = asset_name("9.9.9", "windows-x86_64");
        assert_eq!(file, "recortei-9.9.9-windows-x86_64.exe");
        verify(TEST_KEY, TEST_BIN, TEST_SIG, &file).unwrap();
    }

    #[test]
    fn rejects_tampered_binary() {
        let file = asset_name("9.9.9", "windows-x86_64");
        assert!(verify(TEST_KEY, b"outro conteudo\n", TEST_SIG, &file).is_err());
    }

    #[test]
    fn rejects_signature_for_another_file() {
        // assinatura válida, mas de outra versão: não serve para a 9.9.10
        let file = asset_name("9.9.10", "windows-x86_64");
        assert!(verify(TEST_KEY, TEST_BIN, TEST_SIG, &file).is_err());
    }

    #[test]
    fn rejects_other_key() {
        let file = asset_name("9.9.9", "windows-x86_64");
        assert!(verify(PUBLIC_KEY, TEST_BIN, TEST_SIG, &file).is_err());
    }

    #[test]
    fn embedded_key_parses() {
        let raw = base64::engine::general_purpose::STANDARD
            .decode(PUBLIC_KEY.trim())
            .unwrap();
        minisign_verify::PublicKey::decode(std::str::from_utf8(&raw).unwrap()).unwrap();
    }

    #[test]
    fn versions() {
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(is_newer("0.2.0", "0.1.9"));
        assert!(is_newer("1.0.0", "0.99.99"));
        assert!(is_newer("v0.1.10", "0.1.9"));
        assert!(is_newer("0.2", "0.1.5"));
        assert!(is_newer("0.2.0", "0.2.0-rc1"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.1"));
        assert!(!is_newer("0.2.0-rc1", "0.2.0"));
        assert!(!is_newer("lixo", "0.1.0"));
    }

    #[test]
    fn asset_names() {
        assert_eq!(
            asset_name("0.1.1", "linux-x86_64"),
            "recortei-0.1.1-linux-x86_64"
        );
    }
}
