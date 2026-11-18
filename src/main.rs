//! Recortei — captura de área de tela com anotações, estilo ShareX.
//!
//! - Fica na bandeja do sistema.
//! - O atalho configurado (padrão PrintScreen; global no Windows, atalho do
//!   GNOME no Linux) inicia a captura. Troque em Configurações (bandeja).
//! - Arraste para selecionar a área; depois anote com as ferramentas da barra
//!   (caneta, linha, seta, retângulo, elipse, texto, marca-texto, pixelizar,
//!   numeração).
//! - Enter, Ctrl+C ou duplo clique: copia para a área de transferência e salva.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod config;
mod draw;
#[cfg(target_os = "linux")]
mod gnome;
mod hotkey;
mod output;
mod overlay;
mod platform;
mod settings;
mod setup;
mod text;
mod upload;

const HELP: &str = "Recortei — captura de área de tela com anotações

Uso: recortei [opção]

  (sem opção)   inicia na bandeja; o atalho configurado (padrão:
                PrintScreen) captura
  --capture     captura agora (inicia o app se não estiver rodando)
  --settings    abre as Configurações (atalho de captura)
  --quit        encerra a instância em execução
  --setup       Linux: configura início automático, o atalho configurado
                (padrão: PrintScreen) no GNOME e permissão do portal de
                screenshot (feito sozinho na primeira abertura; pode repetir)
  --unsetup     Linux: desfaz o --setup (início automático, atalho do GNOME)
                e devolve os atalhos de captura do GNOME; use ao desinstalar
  --version     mostra a versão
  --help        mostra esta ajuda";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |s: &str| args.iter().any(|a| a == s);
    if has("--help") || has("-h") || has("--version") || has("-V") {
        platform::attach_console();
        if has("--version") || has("-V") {
            println!("recortei {}", env!("CARGO_PKG_VERSION"));
        } else {
            println!("{HELP}");
        }
        return;
    }
    if has("--setup") {
        std::process::exit(setup::run());
    }
    if has("--unsetup") {
        std::process::exit(setup::unsetup());
    }
    let (cmd, startup) = if has("--quit") {
        ("quit", app::Startup::Idle)
    } else if has("--capture") {
        ("capture", app::Startup::Capture)
    } else if has("--settings") {
        ("settings", app::Startup::Settings)
    } else {
        ("hello", app::Startup::Idle)
    };
    match platform::acquire_or_send(cmd) {
        platform::Instance::Secondary => {}
        platform::Instance::Primary(server) => {
            if cmd == "quit" {
                return; // nada rodando para encerrar
            }
            setup::first_run();
            if let Err(e) = app::run(server, startup) {
                platform::attach_console();
                eprintln!("Recortei: {e}");
                std::process::exit(1);
            }
        }
    }
}
