use std::result::Result;

use alloy::{
    primitives::Address,
    signers::local::{LocalSignerError, MnemonicBuilder, PrivateKeySigner, coins_bip39::English},
};
use bip39::{Language, Mnemonic};
use tracing::*;

use crate::log_err;

#[derive(thiserror::Error, Debug)]
pub enum WalletError {
    #[error("local signer error: {0}")]
    Signer(#[source] LocalSignerError),

    #[error("BIP39 operation error: {0}")]
    Bip39(#[source] bip39::Error),
}

const MNEMONIC_NUM: usize = 12;

pub struct Wallet {}

impl Wallet {
    pub fn create() -> Result<(String, Address), WalletError> {
        let mnemonic = Mnemonic::generate_in(Language::English, MNEMONIC_NUM)
            .map_err(|e| log_err!(WalletError::Bip39(e), "create"))?;
        let mnemonic = mnemonic.words().collect::<Vec<_>>().join(" ");
        let signer = MnemonicBuilder::<English>::default()
            .phrase(&mnemonic)
            .build()
            .map_err(|e| log_err!(WalletError::Signer(e), "create"))?;
        Ok((mnemonic, signer.address()))
    }

    pub fn load(mnemonic: &str) -> Result<PrivateKeySigner, WalletError> {
        let signer: PrivateKeySigner = MnemonicBuilder::<English>::default()
            .phrase(mnemonic)
            .build()
            .map_err(|e| log_err!(WalletError::Signer(e), "load"))?;
        Ok(signer)
    }
}
