use serde::{Deserialize, Serialize};

/// How the local endpoint behaves for incoming requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Mode {
    /// Capture-only fake endpoint: store the request and reply with a stub
    /// success so JanitorAI sees the message as delivered. Nothing is forwarded.
    #[default]
    Capture,
    /// Reverse-proxy mode: forward to the configured upstream unchanged.
    Forward,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Capture => "capture",
            Mode::Forward => "forward",
        }
    }
}

/// Runtime settings loaded from `config.local.toml` (gitignored).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Address for the local proxy. Defaults to loopback.
    pub listen_addr: String,
    /// Upstream base URL for `/v1/*` traffic (forward mode only).
    #[serde(default = "default_upstream")]
    pub upstream_base_url: String,
    /// Endpoint behavior: capture-only (default) or forward.
    #[serde(default)]
    pub mode: Mode,
    /// Directory for captures. Defaults to the OS user-data directory.
    pub data_dir: Option<String>,
}

fn default_upstream() -> String {
    "https://api.openai.com".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1:8817".to_string(),
            upstream_base_url: default_upstream(),
            mode: Mode::Capture,
            data_dir: None,
        }
    }
}

impl Config {
    /// Load `config.local.toml` when it exists; otherwise use the defaults.
    /// Search an explicit directory, then the repo directory, then the user config directory.
    pub fn load() -> Self {
        for candidate in Self::candidate_paths() {
            if let Ok(text) = std::fs::read_to_string(&candidate) {
                match toml::from_str::<Config>(&text) {
                    Ok(cfg) => {
                        tracing::info!(path = %candidate.display(), "loaded config");
                        return cfg;
                    }
                    Err(e) => {
                        tracing::warn!(path = %candidate.display(), error = %e, "bad config file, using defaults")
                    }
                }
            }
        }
        Config::default()
    }

    fn candidate_paths() -> Vec<std::path::PathBuf> {
        let mut paths = vec![std::path::PathBuf::from("config.local.toml")];
        if let Some(dir) = directories::ProjectDirs::from("com", "FilipNone", "JanitorAI Grabber") {
            paths.push(dir.config_dir().join("config.local.toml"));
        }
        paths
    }

    /// Directory that stores captures. It never uses the repo tree.
    pub fn data_dir(&self) -> std::path::PathBuf {
        if let Some(custom) = &self.data_dir {
            return std::path::PathBuf::from(custom);
        }
        if let Some(dir) = directories::ProjectDirs::from("com", "FilipNone", "JanitorAI Grabber") {
            return dir.data_dir().to_path_buf();
        }
        std::env::temp_dir().join("janitorai-grabber")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_loopback_and_off_repo() {
        let cfg = Config::default();
        assert!(cfg.listen_addr.starts_with("127.0.0.1"));
        assert!(cfg.data_dir().is_absolute());
    }

    #[test]
    fn parses_toml() {
        let cfg: Config = toml::from_str(
            r#"
            listen_addr = "127.0.0.1:9999"
            upstream_base_url = "https://example.test"
            "#,
        )
        .unwrap();
        assert_eq!(cfg.listen_addr, "127.0.0.1:9999");
    }
}
