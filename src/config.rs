use serde::{Deserialize, Serialize};

/// Runtime configuration. Loadable from `config.local.toml` (gitignored).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Local proxy bind address. Loopback only by default.
    pub listen_addr: String,
    /// Upstream base URL that /v1/* traffic is forwarded to.
    pub upstream_base_url: String,
    /// Where captures are stored (defaults to OS user-data dir).
    pub data_dir: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1:8817".to_string(),
            upstream_base_url: "https://api.openai.com".to_string(),
            data_dir: None,
        }
    }
}

impl Config {
    /// Load from `config.local.toml` if present, else defaults.
    /// Tries: explicit dir override > repo dir (local only) > user config dir.
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

    /// Where captures live. Never inside the repo tree.
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
