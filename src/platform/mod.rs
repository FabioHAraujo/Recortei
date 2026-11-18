//! Código específico de cada sistema: captura de tela, bandeja, atalho global,
//! instância única e abrir pasta. Cada módulo expõe a mesma interface.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use self::linux::*;

#[cfg(not(any(windows, target_os = "linux")))]
compile_error!("O Recortei suporta Windows e Linux.");

/// Item "Capturar" do menu da bandeja (igual nos dois sistemas).
fn capture_item(hotkey: &str) -> String {
    format!("Capturar área ({hotkey})")
}

/// Dica do ícone da bandeja.
fn tooltip(hotkey: &str) -> String {
    format!("{hotkey} para capturar uma área da tela")
}
