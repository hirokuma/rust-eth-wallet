use anyhow::Result;
use eth_wallet::{EthWallet, config::Config};
use std::path::Path;
use tracing::*;
use tracing_subscriber::{EnvFilter, prelude::*};

#[tokio::main]
async fn main() -> Result<()> {
    let filter = EnvFilter::builder().parse_lossy("info,sample=trace,eth_wallet=trace");
    tracing_subscriber::Registry::default()
        .with(
            tracing_subscriber::fmt::layer()
                .with_file(true)
                .with_line_number(true)
                .with_filter(filter),
        )
        .init();

    let config = Config {
        privkey_path: Path::new("./sample-privkey.txt").to_path_buf(),
        is_poa: false,
        rpc_url: "http://localhost:8545".to_string(),
        rpc_ws: "http://localhost:8545".to_string(),
    };

    let passphrase = "SuperSecurePassword456!";
    let save_privkey = |priv_data: &str, config: &Config| {
        // debug!("mnemonic={}", priv_data);
        eth_wallet::save_encoded_private_key(priv_data, config, passphrase)
    };
    let load_privkey =
        |config: &Config| match eth_wallet::load_encoded_private_key(config, passphrase) {
            Ok(mnemonic) => {
                // debug!("mnemonic={}", mnemonic);
                Ok(mnemonic)
            }
            Err(e) => Err(e),
        };

    let wallet = match config.privkey_path.exists() {
        true => {
            info!("load wallet");
            EthWallet::load(config, load_privkey).await?
        }
        false => {
            info!("create wallet");
            EthWallet::create(config, save_privkey).await?
        }
    };
    info!("address: {}", wallet.address);

    // balance
    let balance = wallet.balance().await?;
    info!("balance={balance}");

    Ok(())
}
