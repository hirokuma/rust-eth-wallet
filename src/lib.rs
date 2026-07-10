mod config;
mod encdec;
mod erc20;
mod network;
mod wallet;

pub use alloy::primitives::{Address, B256, U256, uint};
pub use alloy::rpc::types::TransactionReceipt;
use std::collections::HashMap;
use std::path::Path;
use std::result::Result;
use std::str::FromStr;
use std::sync::Arc;
use tracing::*;

use crate::erc20::{Erc20Error, Erc20Token};
pub use crate::{config::Config, network::BlockNumber};
use crate::{
    config::ConfigError,
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
    Config(#[source] ConfigError),

    #[error("encrypt/decrypt error: {0}")]
    EncDec(#[source] EncDecError),

    #[error("network error: {0}")]
    Network(#[source] NetworkError),

    #[error("wallet error: {0}")]
    Wallet(#[source] WalletError),

    #[error("ERC20 error: {0}")]
    Erc20(#[source] Erc20Error),

    #[error("eth-wallet error: {0}")]
    EthWallet(String),
}

pub fn load_config(config_fname: &Path) -> Result<Config, Error> {
    Config::new(config_fname).map_err(|e| err_log!(Error::Config(e)))
}

/// 拡張秘密鍵をChaCha20Poly1305でファイル保存する
pub fn save_encoded_private_key(
    priv_data: &str,
    config: &Config,
    passphrase: &str,
) -> Result<(), Error> {
    let xprv_str = priv_data.to_string();
    encdec::encrypt_to_file(&config.privkey_path, &xprv_str, passphrase)
        .map_err(|e| err_log!(Error::EncDec(e)))?;
    Ok(())
}

/// save_encoded_private_key()で保存した拡張秘密鍵ファイルを読み込む
pub fn load_encoded_private_key(config: &Config, passphrase: &str) -> Result<String, Error> {
    encdec::decrypt_from_file(&config.privkey_path, passphrase)
        .map_err(|e| err_log!(Error::EncDec(e)))
}

pub struct EthWallet {
    pub config: Config,
    pub network: Network,
    pub address: Address,

    pub tokens: HashMap<Address, Arc<Erc20Token>>,
}

impl EthWallet {
    /// EthWalletを生成する。秘密鍵ファイルがある場合は失敗する。
    pub async fn create(
        config: Config,
        mut privkey_save_callback: impl FnMut(&str, &Config) -> Result<(), Error>,
    ) -> Result<Self, Error> {
        if Path::new(&config.privkey_path).exists() {
            return Err(err_log!(Error::EthWallet(format!(
                "private key file already exist: {}",
                config.privkey_path.to_string_lossy()
            ))));
        }

        let (mnemonic, address) = Wallet::create().map_err(|e| err_log!(Error::Wallet(e)))?;
        privkey_save_callback(&mnemonic, &config)?;

        let signer = Wallet::load(&mnemonic).map_err(|e| err_log!(Error::Wallet(e)))?;
        let network = Network::new(&config, signer)
            .await
            .map_err(|e| err_log!(Error::Network(e)))?;

        Ok(Self {
            config,
            network,
            address,
            tokens: HashMap::new(),
        })
    }

    /// EthWalletをloadする。秘密鍵ファイルがない場合は失敗する。
    pub async fn load(
        config: Config,
        mut privkey_load_callback: impl FnMut(&Config) -> Result<String, Error>,
    ) -> Result<Self, Error> {
        if !Path::new(&config.privkey_path).exists() {
            return Err(err_log!(Error::EthWallet(format!(
                "private key file not exist: {}",
                config.privkey_path.to_string_lossy()
            ))));
        }

        let mnemonic = privkey_load_callback(&config)?;
        let signer = Wallet::load(&mnemonic).map_err(|e| err_log!(Error::Wallet(e)))?;
        let address = signer.address();
        let network = Network::new(&config, signer)
            .await
            .map_err(|e| err_log!(Error::Network(e)))?;

        Ok(Self {
            config,
            network,
            address,
            tokens: HashMap::new(),
        })
    }
}

impl EthWallet {
    /// 残高取得
    pub async fn my_balance(&self) -> Result<U256, Error> {
        self.network
            .balance(self.address)
            .await
            .map_err(|e| err_log!(Error::Network(e)))
    }

    pub async fn balance(&self, address: Address) -> Result<U256, Error> {
        self.network
            .balance(address)
            .await
            .map_err(|e| err_log!(Error::Network(e)))
    }

    /// ブロック番号取得
    pub async fn block_number(&self) -> Result<BlockNumber, Error> {
        self.network
            .block_number()
            .await
            .map_err(|e| err_log!(Error::Network(e)))
    }

    pub async fn send_native_token(
        &self,
        address: Address,
        amount: U256,
    ) -> Result<TransactionReceipt, Error> {
        let tx = self
            .network
            .send_native_token(address, amount)
            .await
            .map_err(|e| err_log!(Error::Network(e)))?;
        self.network
            .receipt_from_tx(tx)
            .await
            .map_err(|e| err_log!(Error::Network(e)))
    }

    pub async fn receipt(&self, tx_hash: B256) -> Result<TransactionReceipt, Error> {
        self.network
            .receipt_from_txhash(tx_hash)
            .await
            .map_err(|e| err_log!(Error::Network(e)))
    }
}

impl EthWallet {
    pub async fn add_token(&mut self, addr_str: &str) -> Result<Arc<Erc20Token>, Error> {
        let contract_addr = from_str(addr_str)?;
        let token = Erc20Token::new(contract_addr, self.network.provider.clone())
            .await
            .map_err(|e| err_log!(Error::Erc20(e)))?;
        let token = Arc::new(token);
        self.tokens.insert(contract_addr, token.clone());
        Ok(token)
    }

    pub fn token(&self, addr_str: &str) -> Result<&Arc<Erc20Token>, Error> {
        let contract_addr = from_str(addr_str)?;
        match self.tokens.get(&contract_addr) {
            Some(token) => Ok(token),
            None => Err(Error::EthWallet(format!("not found: {addr_str}"))),
        }
    }
}

pub fn from_str(addr_str: &str) -> Result<Address, Error> {
    Address::from_str(addr_str).map_err(|e| {
        err_log!(Error::EthWallet(format!(
            "fail convert address({addr_str}): {e}"
        )))
    })
}

// is valid ETH adddress (accept EIP-55 address or not)
pub fn is_valid_address(addr_str: &str) -> bool {
    Address::from_str(addr_str).is_ok()
}

// is valid ETH adddress (accept only EIP-55 address)
pub fn is_checksummed_address(addr_str: &str) -> bool {
    Address::parse_checksummed(addr_str, None).is_ok()
}

// To EIP-55 address string
pub fn checksummed_address(addr_str: &str) -> Result<String, Error> {
    match Address::from_str(addr_str) {
        Ok(v) => Ok(v.to_checksum(None)),
        Err(e) => Err(Error::EthWallet(format!("{e}"))),
    }
}
