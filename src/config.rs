//! Preferências (ferramenta, cor e espessura) em `<config>/recortei/config.json`
//! e pasta onde os recortes são salvos.

use std::path::PathBuf;

use serde_json::{Map, Value};

pub fn config_file() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("recortei")
        .join("config.json")
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
        let map = std::fs::read_to_string(config_file())
            .ok()
            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
            .and_then(|v| match v {
                Value::Object(m) => Some(m),
                _ => None,
            })
            .unwrap_or_default();
        let mut cfg = Config { map };
        cfg.map.remove("last_rect"); // versões antigas lembravam a área
        cfg
    }

    pub fn save(&self) {
        let path = config_file();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match serde_json::to_string_pretty(&Value::Object(self.map.clone())) {
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

    pub fn remember(&mut self, tool: &str, color: usize, width: i32) {
        self.map.insert("tool".into(), Value::from(tool));
        self.map.insert("color".into(), Value::from(color as i64));
        self.map.insert("width".into(), Value::from(width as i64));
        self.save();
    }
}
