//! Preferências (ferramenta, cor, espessura, atalho de captura) em
//! `<config>/recortei/config.json` (mais o servidor de envio) e pasta onde os
//! recortes são salvos.

use std::path::PathBuf;

use serde_json::{Map, Value};

use crate::hotkey::Hotkey;

pub fn config_file() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("recortei")
        .join("config.json")
}

/// `<config>/recortei/uploads.jsonl`: um JSON por envio, com o link para apagar.
pub fn uploads_file() -> PathBuf {
    config_file().with_file_name("uploads.jsonl")
}

/// `~/Imagens/Recortei` (ou o equivalente do sistema).
pub fn save_dir() -> PathBuf {
    dirs::picture_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Recortei")
}

#[derive(Debug, Clone, Default)]
pub struct Config {
    map: Map<String, Value>,
}

impl Config {
    pub fn load() -> Self {
        let mut cfg = Config {
            map: read_map(&config_file()),
        };
        cfg.map.remove("last_rect"); // versões antigas lembravam a área
        cfg
    }

    /// Grava só as chaves indicadas, por cima do que está no disco: o app,
    /// o `recortei --setup` e a thread do atalho mexem no mesmo arquivo, e
    /// cada um só deve escrever o que mudou.
    fn persist(&self, keys: &[&str]) {
        let path = config_file();
        let mut disk = read_map(&path);
        disk.remove("last_rect");
        for &k in keys {
            match self.map.get(k) {
                Some(v) => disk.insert(k.into(), v.clone()),
                None => disk.remove(k),
            };
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match serde_json::to_string_pretty(&Value::Object(disk)) {
            Ok(s) => {
                if let Err(e) = std::fs::write(&path, s) {
                    eprintln!("Erro salvando {}: {e}", path.display());
                }
            }
            Err(e) => eprintln!("Erro serializando a configuração: {e}"),
        }
    }

    pub fn tool(&self) -> Option<&str> {
        self.map.get("tool").and_then(Value::as_str)
    }

    pub fn color(&self) -> Option<i64> {
        self.map.get("color").and_then(Value::as_i64)
    }

    pub fn width(&self) -> Option<i64> {
        self.map.get("width").and_then(Value::as_i64)
    }

    /// Origem do servidor de envio (padrão `https://vkdl.link`); permite usar
    /// um servidor próprio compatível.
    pub fn upload_url(&self) -> String {
        self.str_value("upload_url")
            .unwrap_or_else(|| crate::upload::DEFAULT_URL.into())
    }

    /// Chave de API opcional (vai como `Authorization: Bearer`): dá prazo de
    /// retenção garantido aos envios.
    pub fn upload_api_key(&self) -> Option<String> {
        self.str_value("upload_api_key")
    }

    fn str_value(&self, key: &str) -> Option<String> {
        self.map
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(Into::into)
    }

    /// No Linux, `recortei --setup` já rodou (sozinho na primeira abertura ou
    /// pelo usuário).
    #[cfg(target_os = "linux")]
    pub fn linux_setup_done(&self) -> bool {
        self.map
            .get("linux_setup_done")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// `done = false` faz a próxima abertura configurar de novo (depois do
    /// `recortei --unsetup`).
    #[cfg(target_os = "linux")]
    pub fn set_linux_setup_done(&mut self, done: bool) {
        if done {
            self.map
                .insert("linux_setup_done".into(), Value::from(true));
        } else {
            self.map.remove("linux_setup_done");
        }
        self.persist(&["linux_setup_done"]);
    }

    /// Atalho de captura; PrintScreen se ausente ou inválido.
    pub fn hotkey(&self) -> Hotkey {
        Hotkey::from_config(self.map.get("hotkey").and_then(Value::as_str))
    }

    /// O config.json tem um atalho válido (escolhido ou já adotado)?
    #[cfg(target_os = "linux")]
    pub fn has_hotkey(&self) -> bool {
        self.map
            .get("hotkey")
            .and_then(Value::as_str)
            .is_some_and(|s| Hotkey::parse(s).is_ok())
    }

    pub fn set_hotkey(&mut self, hk: Hotkey) {
        self.map.insert("hotkey".into(), Value::from(hk.label()));
        self.persist(&["hotkey"]);
    }

    /// Listas originais dos atalhos de captura do GNOME (chave → aceleradores)
    /// guardadas antes de o Recortei tirar delas a tecla que passou a usar.
    #[cfg(target_os = "linux")]
    pub fn gnome_saved_bindings(&self) -> std::collections::BTreeMap<String, Vec<String>> {
        self.map
            .get("gnome_saved_bindings")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default()
    }

    #[cfg(target_os = "linux")]
    pub fn set_gnome_saved_bindings(
        &mut self,
        saved: &std::collections::BTreeMap<String, Vec<String>>,
    ) {
        if saved.is_empty() {
            self.map.remove("gnome_saved_bindings");
        } else if let Ok(v) = serde_json::to_value(saved) {
            self.map.insert("gnome_saved_bindings".into(), v);
        }
        self.persist(&["gnome_saved_bindings"]);
    }

    pub fn remember(&mut self, tool: &str, color: usize, width: i32) {
        self.map.insert("tool".into(), Value::from(tool));
        self.map.insert("color".into(), Value::from(color as i64));
        self.map.insert("width".into(), Value::from(width as i64));
        self.persist(&["tool", "color", "width"]);
    }
}

fn read_map(path: &std::path::Path) -> Map<String, Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| match v {
            Value::Object(m) => Some(m),
            _ => None,
        })
        .unwrap_or_default()
}
