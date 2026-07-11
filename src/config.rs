use std::{
    fs::File,
    io::prelude::*,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use thiserror::Error;
use tracing::*;

use crate::log_err;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("I/O error(path={path}): {err_info}: {source}")]
    File {
        path: PathBuf,
        err_info: &'static str,
        #[source]
        source: std::io::Error,
    },

    #[error("TOML parsing error: {0}")]
    Toml(#[source] toml::de::Error),

    #[error("no enabled backend")]
    NoBackend,
}

/// Wallet config
#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    /// Private key file
    pub privkey_path: PathBuf,

    /// true: Proof of Authority(e.g. Besu)
    pub is_poa: bool,

    /// RPC URL
    pub rpc_url: String,

    /// WebSocket RPC
    pub rpc_ws: String,
}

impl Config {
    pub fn new(fname: &Path) -> Result<Config, ConfigError> {
        let mut settings = String::new();
        let mut f = File::open(fname).map_err(|e| {
            log_err!(
                ConfigError::File {
                    path: fname.into(),
                    err_info: "open",
                    source: e,
                },
                "new"
            )
        })?;
        f.read_to_string(&mut settings).map_err(|e| {
            log_err!(
                ConfigError::File {
                    path: fname.into(),
                    err_info: "read_to_string",
                    source: e,
                },
                "new"
            )
        })?;
        let data: Config =
            toml::from_str(&settings).map_err(|e| log_err!(ConfigError::Toml(e), "new"))?;
        Ok(data)
    }
}
