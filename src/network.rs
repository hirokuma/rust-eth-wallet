use std::{result::Result, time::Duration};

use alloy::{
    eips::BlockNumberOrTag,
    network::{Ethereum, EthereumWallet, TransactionBuilder},
    primitives::{Address, B256, U256},
    providers::{
        Identity, PendingTransactionBuilder, PendingTransactionError, Provider, ProviderBuilder,
        RootProvider, WatchTxError,
        fillers::{
            BlobGasFiller, ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller,
            SimpleNonceManager, WalletFiller,
        },
        transport::RpcError,
    },
    rpc::types::{TransactionReceipt, TransactionRequest},
    signers::local::PrivateKeySigner,
    transports::TransportErrorKind,
};
use tracing::*;

use crate::{config::Config, err_log};

// Ethereum RPCの送信タイムアウト
const SEND_TIMEOUT: Duration = Duration::from_secs(30);

// pub struct EstimateFee {
//     pub base_fee: u128,
//     pub max_fee_per_gas: u128,
//     pub max_priority_fee_per_gas: u128,
// }

// with_simple_nonce_management() // TODO: 原因が分からないがnonceがずれるので毎回取得
pub(crate) type EthProvider = FillProvider<
    JoinFill<
        JoinFill<
            JoinFill<
                Identity,
                JoinFill<GasFiller, JoinFill<BlobGasFiller, JoinFill<NonceFiller, ChainIdFiller>>>,
            >,
            NonceFiller<SimpleNonceManager>,
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

    pub async fn balance(&self) -> Result<U256, NetworkError> {
        let balance = self.provider.get_balance(self.address).await.map_err(|e| {
            err_log!(NetworkError::Rpc {
                method: format!("get_balance({})", self.address),
                source: e,
            })
        })?;
        Ok(balance)
    }

    pub async fn block_number(&self) -> Result<BlockNumber, NetworkError> {
        let current = self.provider.get_block_number().await.map_err(|e| {
            err_log!(NetworkError::Rpc {
                method: "get_block_number()".to_string(),
                source: e,
            })
        })?;
        let safe = match self.block_by_number(self.block_tag.safe()).await {
            Ok(v) => v,
            Err(_) => current,
        };
        let finalized = match self.block_by_number(self.block_tag.finalized()).await {
            Ok(v) => v,
            Err(e) => {
                error!("{e}");
                current
            }
        };
        Ok(BlockNumber {
            current,
            safe,
            finalized,
        })
    }

    async fn block_by_number(&self, number: BlockNumberOrTag) -> Result<u64, NetworkError> {
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

    // pub async fn estimate_fee(&self) -> Result<EstimateFee, NetworkError> {
    //     let block_number = self.provider.get_block_number().await.map_err(|e| {
    //         err_log!(NetworkError::Rpc {
    //             method: "get_block_number()".to_string(),
    //             source: e,
    //         })
    //     })?;
    //     let fee_history = self
    //         .provider
    //         .get_fee_history(
    //             block_number,
    //             BlockNumberOrTag::Number(block_number),
    //             &[EIP1559_FEE_ESTIMATION_REWARD_PERCENTILE],
    //         )
    //         .await
    //         .map_err(|e| {
    //             err_log!(NetworkError::Rpc {
    //                 method: format!("get_fee_history({})", block_number),
    //                 source: e,
    //             })
    //         })?;
    //     let base_fee = fee_history.next_block_base_fee().unwrap_or(0);
    //     trace!("next base_fee: {}", base_fee);

    //     let fees = self.provider.estimate_eip1559_fees().await.map_err(|e| {
    //         err_log!(NetworkError::Rpc {
    //             method: "estimate_eip1559_fees()".to_string(),
    //             source: e,
    //         })
    //     })?;
    //     Ok(EstimateFee {
    //         base_fee,
    //         max_fee_per_gas: fees.max_fee_per_gas,
    //         max_priority_fee_per_gas: fees.max_priority_fee_per_gas,
    //     })
    // }

    pub async fn send_native_token(
        &self,
        address: Address,
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
            .with_to(address)
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
