//! The configuration of a tunnel and its file: pod-cloudflare's.

pub use pod_cloudflare::config::*;

/// Read the configuration file at `path`, and hide each secret of it (a
/// `token:`) from the log and from the report of a failure.
pub fn load(path: &std::path::Path) -> anyhow::Result<Config> {
    let config = load_file(path)?;
    for secret in &config.secrets {
        crate::report::hide(secret);
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    /// The `token:` of a configuration file is hidden from the log and
    /// from the report of a failure.
    #[test]
    fn the_token_of_a_file_is_hidden() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yml");
        let id = "ffd86bd5-1bff-456b-8c2c-75a014cdc4c8";
        let token = crate::cloudflare::http2::base64_encode(
            format!(r#"{{"a":"0123456789abcdef","t":"{id}","s":"AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="}}"#)
                .as_bytes(),
        );
        let text = format!("token: {token}\ningress:\n  - service: http://127.0.0.1:1\n");
        std::fs::write(&path, text).unwrap();
        super::load(&path).unwrap();
        let line = crate::report::mask(&format!("the token {token} is in a line"));
        assert!(!line.contains(&token), "{line}");
    }
}
