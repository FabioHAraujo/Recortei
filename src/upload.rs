//! Envio do recorte para o vkdl.link (https://vkdl.link), ou para um servidor
//! próprio compatível (configurável em `upload_url`).
//!
//! API: `POST <upload_url>/v1/upload` em `multipart/form-data` com o campo
//! `file` e, opcionalmente, `Authorization: Bearer <chave>`. A resposta 201 é
//! um JSON com `url`, `delete_url` e `delete_token`.

use std::time::Duration;

use serde::Deserialize;

pub const DEFAULT_URL: &str = "https://vkdl.link";

/// Resultado de um envio bem-sucedido.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Uploaded {
    pub url: String,
    #[serde(default)]
    pub delete_url: String,
    #[serde(default)]
    pub delete_token: String,
}

/// Envia `png` com o nome `filename` para `base` (origem do servidor) e
/// devolve o link público e os dados para apagar depois.
pub fn vkdl(
    png: &[u8],
    filename: &str,
    base: &str,
    api_key: Option<&str>,
) -> Result<Uploaded, String> {
    let endpoint = endpoint(base);
    let boundary = format!("----RecorteiBoundary{:016x}", nonce());
    let body = multipart_body(&boundary, png, filename);
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .user_agent(concat!("Recortei/", env!("CARGO_PKG_VERSION")))
        .http_status_as_error(false)
        .build()
        .into();
    let mut req = agent.post(&endpoint).header(
        "Content-Type",
        &format!("multipart/form-data; boundary={boundary}"),
    );
    if let Some(key) = api_key.map(str::trim).filter(|k| !k.is_empty()) {
        req = req.header("Authorization", &format!("Bearer {key}"));
    }
    let mut resp = req
        .send(&body[..])
        .map_err(|e| format!("falha de conexão com {}: {e}", host(base)))?;
    let status = resp.status();
    let text = resp
        .body_mut()
        .with_config()
        .limit(64 * 1024)
        .read_to_string()
        .map_err(|e| format!("resposta inválida de {}: {e}", host(base)))?;
    parse_response(status.as_u16(), &text)
}

/// `https://vkdl.link` → `https://vkdl.link/v1/upload` (aceita também a URL
/// completa do endpoint).
fn endpoint(base: &str) -> String {
    let base = base.trim().trim_end_matches('/');
    let base = if base.is_empty() { DEFAULT_URL } else { base };
    if base.ends_with("/v1/upload") {
        base.to_owned()
    } else {
        format!("{base}/v1/upload")
    }
}

/// Nome curto do servidor para as mensagens de erro.
fn host(base: &str) -> String {
    let base = base.trim();
    let base = if base.is_empty() { DEFAULT_URL } else { base };
    let rest = base.split_once("://").map_or(base, |(_, r)| r);
    rest.split('/').next().unwrap_or(rest).to_owned()
}

/// 201 traz o JSON do envio; os erros viram mensagens curtas em português.
fn parse_response(status: u16, text: &str) -> Result<Uploaded, String> {
    if (200..300).contains(&status) {
        let up: Uploaded = serde_json::from_str(text.trim())
            .map_err(|e| format!("resposta inesperada do servidor: {e}"))?;
        if !up.url.starts_with("https://") && !up.url.starts_with("http://") {
            return Err("resposta inesperada do servidor: link ausente".into());
        }
        return Ok(up);
    }
    Err(match status {
        413 => "imagem muito grande para o servidor".into(),
        415 => "formato de imagem não aceito pelo servidor".into(),
        429 => "muitos envios seguidos; aguarde um pouco e tente de novo".into(),
        507 => "o servidor está sem espaço no momento".into(),
        401 => "chave de API inválida ou revogada (upload_api_key)".into(),
        _ => {
            let msg = serde_json::from_str::<serde_json::Value>(text.trim())
                .ok()
                .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_owned))
                .map(|m| m.chars().take(200).collect::<String>());
            match msg {
                Some(m) if !m.is_empty() => format!("o servidor respondeu HTTP {status}: {m}"),
                _ => format!("o servidor respondeu HTTP {status}"),
            }
        }
    })
}

fn multipart_body(boundary: &str, png: &[u8], filename: &str) -> Vec<u8> {
    let mut b = Vec::with_capacity(png.len() + 256);
    let filename: String = filename
        .chars()
        .filter(|c| !matches!(c, '"' | '\r' | '\n' | '\\'))
        .collect();
    b.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
             Content-Type: image/png\r\n\r\n"
        )
        .as_bytes(),
    );
    b.extend_from_slice(png);
    b.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    b
}

/// Valor pseudoaleatório para o delimitador do multipart.
fn nonce() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos()),
    );
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_has_file_field() {
        let body = multipart_body("XYZ", b"\x89PNG", "recortei-1.png");
        let s = String::from_utf8_lossy(&body);
        assert!(s.starts_with(
            "--XYZ\r\nContent-Disposition: form-data; name=\"file\"; filename=\"recortei-1.png\"\r\nContent-Type: image/png\r\n\r\n\u{FFFD}PNG\r\n"
        ));
        assert!(s.ends_with("\r\n--XYZ--\r\n"));

        let s = String::from_utf8_lossy(&multipart_body("B", b"x", "a\"b.png")).into_owned();
        assert!(s.contains("filename=\"ab.png\""));
    }

    #[test]
    fn builds_endpoint() {
        assert_eq!(endpoint("https://vkdl.link"), "https://vkdl.link/v1/upload");
        assert_eq!(
            endpoint("https://img.exemplo.com/ "),
            "https://img.exemplo.com/v1/upload"
        );
        assert_eq!(
            endpoint("http://localhost:8080/v1/upload"),
            "http://localhost:8080/v1/upload"
        );
        assert_eq!(endpoint(""), "https://vkdl.link/v1/upload");
        assert_eq!(host("https://vkdl.link/x"), "vkdl.link");
    }

    #[test]
    fn parses_responses() {
        let ok = r#"{"id":"aB3dE9xQ","url":"https://vkdl.link/aB3dE9xQ.png","delete_url":"https://vkdl.link/v1/files/aB3dE9xQ","delete_token":"tok","size":1,"mime":"image/png","expires_at":null}"#;
        assert_eq!(
            parse_response(201, ok),
            Ok(Uploaded {
                url: "https://vkdl.link/aB3dE9xQ.png".into(),
                delete_url: "https://vkdl.link/v1/files/aB3dE9xQ".into(),
                delete_token: "tok".into(),
            })
        );
        assert!(parse_response(201, "não é json").is_err());
        assert!(parse_response(201, r#"{"url":"ftp://x"}"#).is_err());
        assert!(parse_response(413, "")
            .unwrap_err()
            .contains("muito grande"));
        assert!(parse_response(415, "").unwrap_err().contains("formato"));
        assert!(parse_response(429, "").unwrap_err().contains("aguarde"));
        assert!(parse_response(507, "").unwrap_err().contains("sem espaço"));
        assert_eq!(
            parse_response(500, "").unwrap_err(),
            "o servidor respondeu HTTP 500"
        );
        assert!(
            parse_response(400, r#"{"error":"no_file","message":"sem arquivo"}"#)
                .unwrap_err()
                .contains("sem arquivo")
        );
    }

    /// Envio real; roda só com `RECORTEI_TEST_UPLOAD=1 cargo test -- --ignored`
    /// (servidor em `RECORTEI_TEST_UPLOAD_URL`, padrão https://vkdl.link).
    #[test]
    #[ignore]
    fn real_upload() {
        if std::env::var_os("RECORTEI_TEST_UPLOAD").is_none() {
            return;
        }
        let base = std::env::var("RECORTEI_TEST_UPLOAD_URL").unwrap_or_else(|_| DEFAULT_URL.into());
        let mut pm = tiny_skia::Pixmap::new(8, 8).unwrap();
        pm.fill(tiny_skia::Color::from_rgba8(30, 120, 240, 255));
        let png = pm.encode_png().unwrap();
        let up = vkdl(&png, "recortei-teste.png", &base, None).expect("envio");
        assert!(up.url.starts_with(base.trim_end_matches('/')), "{}", up.url);
        println!("{}", up.url);
    }
}
