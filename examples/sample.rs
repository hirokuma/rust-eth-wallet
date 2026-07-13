use std::{path::Path, process::Command};

use anyhow::Result;
use eth_wallet::{Config, EthWallet, uint};
use tracing::*;
use tracing_subscriber::{EnvFilter, prelude::*};
use wallet_utils::encdec;

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

    let owner = eth_wallet::from_str("0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266")?;
    let config = Config {
        privkey_path: Path::new("./sample-privkey.txt").to_path_buf(),
        is_poa: false,
        rpc_url: "http://localhost:8545".to_string(),
        rpc_ws: "http://localhost:8545".to_string(),
    };

    let passphrase = "SuperSecurePassword456!";
    let save_privkey = |path: &Path, priv_data: &str| {
        // debug!("mnemonic={}", priv_data);
        encdec::save_encoded_private_key(path, priv_data, passphrase)
    };
    let load_privkey = |path: &Path| match encdec::load_encoded_private_key(path, passphrase) {
        Ok(mnemonic) => {
            // debug!("mnemonic={}", mnemonic);
            Ok(mnemonic)
        }
        Err(e) => Err(e),
    };

    let mut wallet = match config.privkey_path.exists() {
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
    let balance = wallet.balance(owner).await?;
    info!("owner balance={balance}");
    let balance = wallet.my_balance().await?;
    info!("balance={balance}");

    if balance < uint!(1_000_000_000_000_000_U256) {
        // owner: send native token
        let output = Command::new("cast")
            .arg("send")
            .arg(wallet.address.to_string())
            .arg("--value")
            .arg("1ether")
            .arg("--private-key")
            .arg("0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80")
            .output()
            .expect("cast send");
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("Error happened:\n{}", stderr);
        }

        // balance
        let balance = wallet.balance(owner).await?;
        info!("owner balance={balance}");
        let balance = wallet.my_balance().await?;
        info!("balance={balance}");
    }

    // ERC-20
    let contract = "0x9fe46736679d2d9a65f0992f2272de9f3c7fa6e0";
    let contract = eth_wallet::from_str(contract)?;
    let token = wallet.add_token(contract).await?;
    info!("token: {:#?}", token);
    let balance = token.balance_of(owner).await?;
    info!("owner balance: {}", balance);
    let balance = token.balance_of(wallet.address).await?;
    info!("token balance: {}", balance);

    if balance < uint!(1_000_000_000_U256) {
        // owner: send ERC20 token
        let output = Command::new("cast")
            .arg("send")
            .arg(token.address.to_string())
            .arg("transfer(address,uint256)(bool)")
            .arg(wallet.address.to_string())
            .arg("1000000000")
            .arg("--private-key")
            .arg("0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80")
            .output()
            .expect("cast send");
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("Error happened:\n{}", stderr);
        }

        let balance = token.balance_of(owner).await?;
        info!("owner balance: {}", balance);
        let balance = token.balance_of(wallet.address).await?;
        info!("token balance: {}", balance);
    }

    let tx_hash = token.transfer(owner, uint!(1_000_U256)).await?;
    let receipt = wallet.receipt(tx_hash).await?;
    debug!("receipt: {:?}", receipt);

    let balance = token.balance_of(owner).await?;
    info!("owner balance: {}", balance);
    let balance = token.balance_of(wallet.address).await?;
    info!("token balance: {}", balance);

    Ok(())
}
