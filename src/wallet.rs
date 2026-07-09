use std::result::Result;

use alloy::{
    primitives::Address,
    signers::local::{LocalSignerError, MnemonicBuilder, PrivateKeySigner, coins_bip39::English},
};
use bip39::{Language, Mnemonic};

#[derive(thiserror::Error, Debug)]
pub enum WalletError {
    #[error("local signer error: {0}")]
    File(#[from] LocalSignerError),

    #[error("BIP39 operation: {0}")]
    Bip39(#[from] bip39::Error),
}

const MNEMONIC_NUM: usize = 12;

pub struct Wallet {}

impl Wallet {
    pub fn create() -> Result<(String, Address), WalletError> {
        let mnemonic = Mnemonic::generate_in(Language::English, MNEMONIC_NUM)?;
        let mnemonic = mnemonic.words().collect::<Vec<_>>().join(" ");
        let signer = MnemonicBuilder::<English>::default()
            .phrase(&mnemonic)
            .build()?;
        Ok((mnemonic, signer.address()))
    }

    pub fn load(mnemonic: &str) -> Result<PrivateKeySigner, WalletError> {
        let signer: PrivateKeySigner = MnemonicBuilder::<English>::default()
            .phrase(mnemonic)
            .build()?;
        Ok(signer)
    }
}
