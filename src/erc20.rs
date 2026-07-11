use std::result::Result;

use alloy::{
    hex::FromHexError,
    primitives::{Address, B256, Log, U256, ruint::ParseError},
    rpc::types::TransactionReceipt,
    sol,
    sol_types::SolEvent,
};
use tracing::*;

use crate::{log_err, network::EthProvider};

#[derive(thiserror::Error, Debug)]
pub enum Erc20Error {
    #[error("contract error: {contract}: {source}")]
    Contract {
        contract: String,
        #[source]
        source: alloy::contract::Error,
    },

    #[error("contract error: {0}")]
    HexConvert(#[source] FromHexError),

    #[error("contract error: {0}")]
    Parse(#[source] ParseError),
}

sol! {
    // https://eips.ethereum.org/EIPS/eip-20
    #[sol(rpc)]
    contract Erc20 {
        function name() public view returns (string);
        function symbol() public view returns (string);
        function decimals() public view returns (uint8);
        function totalSupply() public view returns (uint256);
        function balanceOf(address account) public view returns (uint256);
        function transfer(address to, uint256 value) public returns (bool);
        function transferFrom(address sender, address recipient, uint256 amount)
            public
            returns (bool);
        function approve(address spender, uint256 amount) public returns (bool);
        function allowance(address owner, address spender)
            public
            view
            returns (uint256);

        event Transfer(address indexed from, address indexed to, uint256 value);
        event Approval(address indexed owner, address indexed spender, uint256 value);
    }
}

pub type Erc20Contract = Erc20::Erc20Instance<EthProvider>;

#[derive(Debug)]
pub struct Erc20Token {
    pub address: Address,
    pub name: String,
    pub symbol: String,
    pub total_supply: U256,
    token: Erc20Contract,
}

impl Erc20Token {
    pub async fn new(address: Address, network: EthProvider) -> Result<Self, Erc20Error> {
        let token = Erc20::new(address, network);
        let name = token.name().call().await.map_err(|e| {
            log_err!(
                Erc20Error::Contract {
                    contract: "name".to_string(),
                    source: e
                },
                "new"
            )
        })?;
        let symbol = token.symbol().call().await.map_err(|e| {
            log_err!(
                Erc20Error::Contract {
                    contract: "symbol".to_string(),
                    source: e
                },
                "new"
            )
        })?;
        let total_supply = token.totalSupply().call().await.map_err(|e| {
            log_err!(
                Erc20Error::Contract {
                    contract: "total_supply".to_string(),
                    source: e
                },
                "new"
            )
        })?;

        Ok(Self {
            address,
            name,
            symbol,
            total_supply,
            token,
        })
    }

    pub async fn balance_of(&self, address: Address) -> Result<U256, Erc20Error> {
        let balance = self.token.balanceOf(address).call().await.map_err(|e| {
            log_err!(
                Erc20Error::Contract {
                    contract: format!("balanceOf({})", address),
                    source: e
                },
                "balance_of"
            )
        })?;
        Ok(balance)
    }

    pub async fn transfer(&self, to_addr: Address, amount: U256) -> Result<B256, Erc20Error> {
        let tx = self
            .token
            .transfer(to_addr, amount)
            .send()
            .await
            .map_err(|e| {
                log_err!(
                    Erc20Error::Contract {
                        contract: format!("transfer({}, {})", to_addr, amount),
                        source: e,
                    },
                    "transfer"
                )
            })?;
        Ok(*tx.tx_hash())
    }

    // コントラクトへのTransfer event
    pub async fn get_transfer_event(
        &self,
        receipt: TransactionReceipt,
    ) -> Result<Vec<Log<Erc20::Transfer>>, Erc20Error> {
        let logs = receipt.inner.logs();
        let events: Vec<Log<Erc20::Transfer>> = logs
            .iter()
            .filter_map(|log| Erc20::Transfer::decode_log(log.as_ref()).ok())
            .filter(|log| log.to == *self.token.address())
            .collect();
        for event in events.iter() {
            info!(
                "Transfer event log: from={}, to={}, value={}",
                event.from, event.to, event.value
            );
        }
        Ok(events)
    }
}
