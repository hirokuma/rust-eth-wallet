use std::{result::Result, time::Duration};

use alloy::{
    eips::BlockNumberOrTag,
    network::{Ethereum, EthereumWallet, TransactionBuilder},
    primitives::{Address, B256, U256},
    providers::{ProviderBuilder, RootProvider},
    rpc::types::{TransactionReceipt, TransactionRequest},
    signers::local::PrivateKeySigner,
    transports::TransportErrorKind,
};
use alloy_provider::{
    Identity, PendingTransactionBuilder, PendingTransactionError, Provider, WatchTxError,
    fillers::{
        BlobGasFiller, ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller, WalletFiller,
    },
};
use alloy_transport::RpcError;
use tracing::*;

use crate::{config::Config, err_log};

// Ethereum RPCの送信タイムアウト
const SEND_TIMEOUT: Duration = Duration::from_secs(30);

// pub(crate) type EthProvider = FillProvider<
//     JoinFill<
//         JoinFill<
//             Identity,
//             JoinFill<
//                 GasFiller,
//                 JoinFill<
//                     BlobGasFiller,
//                     JoinFill<NonceFiller, ChainIdFiller>
//                 >
//             >
//         >,
//         WalletFiller<EthereumWallet>
//     >,
//     RootProvider
// >;

// with_simple_nonce_management() // TODO: 原因が分からないがnonceがずれるので毎回取得
pub(crate) type EthProvider = FillProvider<
    JoinFill<
        JoinFill<
            JoinFill<
                Identity,
                JoinFill<GasFiller, JoinFill<BlobGasFiller, JoinFill<NonceFiller, ChainIdFiller>>>,
            >,
            NonceFiller<alloy_provider::fillers::SimpleNonceManager>,
        >,
        WalletFiller<EthereumWallet>,
    >,
    RootProvider,
>;

#[derive(thiserror::Error, Debug)]
pub enum NetworkError {
    #[error("new instance: RPC_URL={rpc_url}: {source}")]
    New {
        rpc_url: String,
        #[source]
        source: RpcError<TransportErrorKind>,
    },

    #[error("call RPC(method={method}): {source}")]
    Rpc {
        method: String,
        #[source]
        source: RpcError<TransportErrorKind>,
    },

    #[error("RPC timeout(method={method}): {source}")]
    Timeout {
        method: String,
        #[source]
        source: PendingTransactionError,
    },

    #[error("pending transaction error(method={method}): {source}")]
    PendingTx {
        method: String,
        #[source]
        source: PendingTransactionError,
    },

    #[error("pending transaction error(method={method}): {source}")]
    Receipt {
        method: String,
        #[source]
        source: PendingTransactionError,
    },

    #[error("call RPC(method={method})")]
    Network { method: String },
}

pub struct Network {
    pub address: Address,
    pub signer: PrivateKeySigner,
    pub provider: EthProvider,
    pub block_tag: BlockTag,
}

pub struct BlockNumber {
    pub current: u64,
    pub safe: u64,
    pub finalized: u64,
}

pub struct BlockTag {
    safe: BlockNumberOrTag,
    finalized: BlockNumberOrTag,
}
impl BlockTag {
    pub fn new(is_poa: bool) -> Self {
        if is_poa {
            // PoAのときはブロック取得でSafeやFinalizedが使用できない
            Self {
                safe: BlockNumberOrTag::Latest,
                finalized: BlockNumberOrTag::Latest,
            }
        } else {
            Self {
                safe: BlockNumberOrTag::Safe,
                finalized: BlockNumberOrTag::Finalized,
            }
        }
    }

    pub fn safe(&self) -> BlockNumberOrTag {
        self.safe
    }
    pub fn finalized(&self) -> BlockNumberOrTag {
        self.finalized
    }
}

impl Network {
    pub async fn new(config: &Config, signer: PrivateKeySigner) -> Result<Network, NetworkError> {
        let addr = signer.address();
        let provider = ProviderBuilder::new()
            .with_simple_nonce_management() // TODO: 原因が分からないがnonceがずれるので毎回取得
            .wallet(signer.clone())
            .connect(&config.rpc_url)
            .await
            .map_err(|e| NetworkError::New {
                rpc_url: config.rpc_url.clone(),
                source: e,
            })?;
        Ok(Network {
            address: addr,
            signer,
            provider,
            block_tag: BlockTag::new(config.is_poa),
        })
    }

    pub async fn get_balance(&self) -> Result<U256, NetworkError> {
        let balance = self.provider.get_balance(self.address).await.map_err(|e| {
            err_log!(NetworkError::Rpc {
                method: format!("get_balance({})", self.address),
                source: e,
            })
        })?;
        trace!("get_balance: {balance}");
        Ok(balance)
    }

    pub async fn get_block_number(&self) -> Result<BlockNumber, NetworkError> {
        let current = self.provider.get_block_number().await.map_err(|e| {
            err_log!(NetworkError::Rpc {
                method: "get_block_number()".to_string(),
                source: e,
            })
        })?;
        let safe = match self.get_block_by_number(self.block_tag.safe()).await {
            Ok(v) => v,
            Err(_) => current,
        };
        let finalized = match self.get_block_by_number(self.block_tag.finalized()).await {
            Ok(v) => v,
            Err(e) => {
                error!("{e}");
                current
            }
        };
        trace!("get_block_number: {}/{}/{}", current, safe, finalized);
        Ok(BlockNumber {
            current,
            safe,
            finalized,
        })
    }

    async fn get_block_by_number(&self, number: BlockNumberOrTag) -> Result<u64, NetworkError> {
        match self.provider.get_block_by_number(number).await {
            Ok(v) => match v {
                Some(block) => Ok(block.number()),
                None => Err(err_log!(NetworkError::Network {
                    method: format!("get_block_by_number({}) is none", number),
                })),
            },
            Err(e) => Err(err_log!(NetworkError::Network {
                method: format!("get_block_by_number({}) is err: {e}", number),
            })),
        }
    }

    // pub(crate) async fn estimate_fee(&self) -> Result<(u128, u128), EthError> {
    //     trace!("estimate_fee");
    //     let block_number = self.provider.get_block_number().await?;
    //     let fee_history = self
    //         .provider
    //         .get_fee_history(
    //             block_number,
    //             BlockNumberOrTag::Number(block_number),
    //             &[alloy_provider::utils::EIP1559_FEE_ESTIMATION_REWARD_PERCENTILE],
    //         )
    //         .await?;
    //     let base_fee = fee_history.next_block_base_fee().unwrap_or(0);
    //     trace!("next base_fee: {}", base_fee);

    //     let fees = self.provider.estimate_eip1559_fees().await?;
    //     trace!("estimate max_fee_per_gas: {}", fees.max_fee_per_gas);
    //     trace!(
    //         "estimate max_priority_fee_per_gas: {}",
    //         fees.max_priority_fee_per_gas
    //     );
    //     if fees.max_fee_per_gas < base_fee + fees.max_priority_fee_per_gas {
    //         Ok((
    //             fees.max_fee_per_gas,
    //             base_fee + fees.max_priority_fee_per_gas,
    //         ))
    //     } else {
    //         Ok((
    //             base_fee + fees.max_priority_fee_per_gas,
    //             fees.max_fee_per_gas,
    //         ))
    //     }
    // }

    pub async fn send_transaction(
        &self,
        addr: Address,
        amount: U256,
    ) -> Result<PendingTransactionBuilder<Ethereum>, NetworkError> {
        let fees = self.provider.estimate_eip1559_fees().await.map_err(|e| {
            err_log!(NetworkError::Rpc {
                method: "estimate_eip1559_fees()".to_string(),
                source: e,
            })
        })?;
        let tx = TransactionRequest::default()
            .with_max_fee_per_gas(fees.max_fee_per_gas)
            .with_max_priority_fee_per_gas(fees.max_priority_fee_per_gas)
            .with_to(addr)
            .with_value(amount);
        let tx = self
            .provider
            .send_transaction(tx.clone())
            .await
            .map_err(|e| {
                err_log!(NetworkError::Rpc {
                    method: format!("send_transaction({:?})", tx.clone()),
                    source: e,
                })
            })?;
        Ok(tx)
    }

    pub async fn get_receipt_from_tx(
        &self,
        tx: PendingTransactionBuilder<Ethereum>,
    ) -> Result<TransactionReceipt, NetworkError> {
        match tx.with_timeout(Some(SEND_TIMEOUT)).get_receipt().await {
            Ok(receipt) => Ok(receipt),
            Err(e @ PendingTransactionError::TxWatcher(WatchTxError::Timeout)) => {
                Err(err_log!(NetworkError::Timeout {
                    method: "get_receipt_from_tx".to_string(),
                    source: e
                }))
            }
            Err(e) => Err(err_log!(NetworkError::PendingTx {
                method: "get_receipt_from_tx".to_string(),
                source: e
            })),
        }
    }

    pub async fn get_receipt_from_txhash(
        &self,
        txhash: B256,
    ) -> Result<TransactionReceipt, NetworkError> {
        match self
            .provider
            .get_transaction_receipt(txhash)
            .await
            .map_err(|e| {
                err_log!(NetworkError::Rpc {
                    method: format!("get_transaction_receipt({})", txhash),
                    source: e,
                })
            })? {
            Some(v) => Ok(v),
            None => Err(NetworkError::Network {
                method: "transaction receipt not found".to_string(),
            }),
        }
    }
}
