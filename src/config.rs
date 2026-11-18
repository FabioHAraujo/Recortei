//! Preferências (ferramenta, cor e espessura) em `<config>/recortei/config.json`
//! (mais o servidor de envio) e pasta onde os recortes são salvos.

use std::path::PathBuf;

use serde_json::{Map, Value};

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

    #[cfg(target_os = "linux")]
    pub fn set_linux_setup_done(&mut self) {
        self.map
            .insert("linux_setup_done".into(), Value::from(true));
        self.save();
    }

    pub fn remember(&mut self, tool: &str, color: usize, width: i32) {
        self.map.insert("tool".into(), Value::from(tool));
        self.map.insert("color".into(), Value::from(color as i64));
        self.map.insert("width".into(), Value::from(width as i64));
        self.save();
    }
}
