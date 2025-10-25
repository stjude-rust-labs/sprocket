//! Server configuration.

use std::path::PathBuf;

use anyhow::Context;
use serde::Deserialize;

/// Default host.
const DEFAULT_HOST: &str = "127.0.0.1";

/// Default port.
const DEFAULT_PORT: u16 = 8080;

/// Default max database connections.
const DEFAULT_MAX_CONNECTIONS: u32 = 20;

/// Default database URL (in-memory SQLite database).
const DEFAULT_DATABASE_URL: &str = "sqlite::memory:";

/// Default runs directory.
const DEFAULT_RUNS_DIRECTORY: &str = "./runs";

/// Default runs directory function for serde.
fn default_runs_directory() -> PathBuf {
    PathBuf::from(DEFAULT_RUNS_DIRECTORY)
}

/// Server configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Server settings.
    pub server: ServerConfig,
    /// Database settings.
    pub database: DatabaseConfig,
}

/// Server-specific configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Host to bind to (default: `127.0.0.1`).
    #[serde(default)]
    pub host: String,
    /// Port to bind to (default: `8080`).
    #[serde(default)]
    pub port: u16,
    /// Allow file-based WDL sources (default: `false`).
    #[serde(default)]
    pub allow_file_sources: bool,
    /// Allowed file paths when `allow_file_sources` is `true`.
    #[serde(default)]
    pub allowed_file_paths: Vec<PathBuf>,
    /// Maximum concurrent workflows (default: `None` - no limit).
    #[serde(default)]
    pub max_concurrent_workflows: Option<usize>,
    /// Directory for workflow execution runs (default: `./runs`).
    #[serde(default = "default_runs_directory")]
    pub runs_directory: PathBuf,
}

/// Database configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    /// Database URL (e.g., `sqlite://sprocket.db` or `postgresql://...`).
    pub url: String,
    /// Maximum database connections (default: `20`).
    #[serde(default)]
    pub max_connections: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: String::from(DEFAULT_HOST),
            port: DEFAULT_PORT,
            allow_file_sources: false,
            allowed_file_paths: vec![],
            max_concurrent_workflows: None,
            runs_directory: default_runs_directory(),
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: String::from(DEFAULT_DATABASE_URL),
            max_connections: DEFAULT_MAX_CONNECTIONS,
        }
    }
}

impl Config {
    /// Load configuration from a TOML file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)?;
        let mut config: Config = toml::from_str(&contents)?;

        // Canonicalize allowed file paths.
        config.server.allowed_file_paths = config
            .server
            .allowed_file_paths
            .into_iter()
            .map(|p| {
                p.canonicalize()
                    .context(format!("failed to canonicalize allowed path: {}", p.display()))
            })
            .collect::<Result<Vec<_>, _>>()?;

        config.validate()?;
        Ok(config)
    }

    /// Validate configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if configuration is invalid.
    fn validate(&self) -> anyhow::Result<()> {
        if self.server.allow_file_sources && self.server.allowed_file_paths.is_empty() {
            anyhow::bail!(
                "`allow_file_sources` is `true` but `allowed_file_paths` is empty"
            );
        }
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            database: DatabaseConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.server.host, "127.0.0.1");
        assert_eq!(config.server.port, 8080);
        assert!(!config.server.allow_file_sources);
        assert!(config.server.allowed_file_paths.is_empty());
        assert!(config.server.max_concurrent_workflows.is_none());
        assert_eq!(config.database.url, "sqlite::memory:");
        assert_eq!(config.database.max_connections, 20);
    }

    #[test]
    fn test_validate_file_sources_enabled_without_paths() {
        let config = Config {
            server: ServerConfig {
                allow_file_sources: true,
                allowed_file_paths: vec![],
                ..Default::default()
            },
            database: DatabaseConfig::default(),
        };

        let result = config.validate();
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().to_string(),
            "`allow_file_sources` is `true` but `allowed_file_paths` is empty"
        );
    }

    #[test]
    fn test_validate_file_sources_enabled_with_paths() {
        let config = Config {
            server: ServerConfig {
                allow_file_sources: true,
                allowed_file_paths: vec![PathBuf::from("/tmp")],
                ..Default::default()
            },
            database: DatabaseConfig::default(),
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_file_sources_disabled() {
        let config = Config {
            server: ServerConfig {
                allow_file_sources: false,
                allowed_file_paths: vec![],
                ..Default::default()
            },
            database: DatabaseConfig::default(),
        };

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_from_toml() {
        let toml = r#"
            [server]
            host = "0.0.0.0"
            port = 9000
            allow_file_sources = true
            allowed_file_paths = ["/workflows", "/data"]
            max_concurrent_workflows = 5

            [database]
            url = "sqlite://test.db"
            max_connections = 10
        "#;

        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.server.host, "0.0.0.0");
        assert_eq!(config.server.port, 9000);
        assert!(config.server.allow_file_sources);
        assert_eq!(config.server.allowed_file_paths.len(), 2);
        assert_eq!(config.server.max_concurrent_workflows, Some(5));
        assert_eq!(config.database.url, "sqlite://test.db");
        assert_eq!(config.database.max_connections, 10);
    }
}
