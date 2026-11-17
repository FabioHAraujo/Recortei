//! Recortei — captura de área de tela com anotações, estilo ShareX.
//!
//! - Fica na bandeja do sistema.
//! - Tecla Insert (global no Windows; atalho do GNOME no Linux) inicia a captura.
//! - Arraste para selecionar a área; depois anote com as ferramentas da barra
//!   (caneta, linha, seta, retângulo, elipse, texto, marca-texto, pixelizar,
//!   numeração).
//! - Enter, Ctrl+C ou duplo clique: copia para a área de transferência e salva.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod config;
mod draw;
mod output;
mod overlay;
mod platform;
mod text;

const HELP: &str = "Recortei — captura de área de tela com anotações

Uso: recortei [opção]

  (sem opção)   inicia na bandeja (Insert captura)
  --capture     captura agora (inicia o app se não estiver rodando)
  --quit        encerra a instância em execução
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
    let cmd = if has("--quit") {
        "quit"
    } else if has("--capture") {
        "capture"
    } else {
        "hello"
    };
    match platform::acquire_or_send(cmd) {
        platform::Instance::Secondary => {}
        platform::Instance::Primary(server) => {
            if cmd == "quit" {
                return; // nada rodando para encerrar
            }
            if let Err(e) = app::run(server, cmd == "capture") {
                platform::attach_console();
                eprintln!("Recortei: {e}");
                std::process::exit(1);
            }
        }
    }
}
