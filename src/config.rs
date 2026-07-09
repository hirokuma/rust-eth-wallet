use std::{fs::File, io::prelude::*, path::PathBuf};

use serde::Deserialize;
use thiserror::Error;
use tracing::*;

use crate::err_log;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("I/O error({source}): {err_info}")]
    File {
        path: PathBuf,
        err_info: &'static str,
        #[source]
        source: std::io::Error,
    },

    #[error("TOML parsing error({source})")]
    Toml {
        #[source]
        source: toml::de::Error,
    },

    #[error("no enabled backend")]
    NoBackend,
}

/// Wallet config
#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    /// Private key filename
    pub privkey_fname: PathBuf,

    /// true: PoA(e.g. Besu)
    pub is_poa: bool,

    /// RPC URL
    pub rpc_url: String,

    /// WebSocket RPC
    pub rpc_ws: String,
}

impl Config {
    pub fn new(fname: &str) -> Result<Config, ConfigError> {
        let mut settings = String::new();
        let mut f = File::open(fname).map_err(|e| {
            err_log!(ConfigError::File {
                path: fname.into(),
                err_info: "open",
                source: e,
            })
        })?;
        f.read_to_string(&mut settings).map_err(|e| {
            err_log!(ConfigError::File {
                path: fname.into(),
                err_info: "read_to_string",
                source: e,
            })
        })?;
        let data: Config =
            toml::from_str(&settings).map_err(|e| err_log!(ConfigError::Toml { source: e }))?;
        Ok(data)
    }
}
