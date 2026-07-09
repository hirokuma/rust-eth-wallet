pub mod config;
mod encdec;
mod network;
mod wallet;

pub use alloy::primitives::Address;
use std::result::Result;
use tracing::*;

use crate::{
    config::{Config, ConfigError},
    encdec::EncDecError,
    network::{Network, NetworkError},
    wallet::{Wallet, WalletError},
};

#[macro_export]
macro_rules! err_log {
    ($err_variant:expr) => {{
        let err = $err_variant;
        error!("{err}");
        err
    }};
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("config error: {0}")]
    Config(#[from] ConfigError),

    #[error("encrypt/decrypt: {0}")]
    EncDec(#[from] EncDecError),

    #[error("network error: {0}")]
    Network(#[from] NetworkError),

    #[error("wallet operation: {0}")]
    Wallet(#[from] WalletError),
}

pub fn load_config(config_fname: &str) -> Result<Config, Error> {
    Ok(Config::new(config_fname)?)
}

/// 拡張秘密鍵をChaCha20Poly1305でファイル保存する
pub fn save_encoded_private_key(
    priv_data: &str,
    config: &Config,
    passphrase: &str,
) -> Result<(), Error> {
    let xprv_str = priv_data.to_string();
    encdec::encrypt_to_file(&config.privkey_fname, &xprv_str, passphrase)?;
    Ok(())
}

/// save_encoded_private_key()で保存した拡張秘密鍵ファイルを読み込む
pub fn load_encoded_private_key(config: &Config, passphrase: &str) -> Result<String, Error> {
    Ok(encdec::decrypt_from_file(
        &config.privkey_fname,
        passphrase,
    )?)
}

pub struct EthWallet {
    pub config: Config,
    pub network: Network,
    pub address: Address,
}

impl EthWallet {
    /// EthWalletを生成する。秘密鍵ファイルがある場合は失敗する。
    pub async fn create(
        config: Config,
        mut privkey_save_callback: impl FnMut(&str, &Config) -> Result<(), Error>,
    ) -> Result<Self, Error> {
        let (mnemonic, address) = Wallet::create()?;
        privkey_save_callback(&mnemonic, &config)?;

        let signer = Wallet::load(&mnemonic)?;
        let network = Network::new(&config, signer).await?;
        let balance = network.get_balance().await?;
        trace!("balance={balance}");

        Ok(Self {
            config,
            network,
            address,
        })
    }

    /// EthWalletをloadする。秘密鍵ファイルがない場合は失敗する。
    pub async fn load(
        config: Config,
        mut privkey_load_callback: impl FnMut(&Config) -> Result<String, Error>,
    ) -> Result<Self, Error> {
        let mnemonic = privkey_load_callback(&config)?;
        let signer = Wallet::load(&mnemonic)?;
        let address = signer.address();
        let network = Network::new(&config, signer).await?;
        let balance = network.get_balance().await?;
        trace!("balance={balance}");

        Ok(Self {
            config,
            network,
            address,
        })
    }
}
