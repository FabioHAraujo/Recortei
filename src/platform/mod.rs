//! Código específico de cada sistema: captura de tela, bandeja, tecla global,
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
