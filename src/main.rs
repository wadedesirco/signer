extern crate hex;
use hex::encode;
use std::fmt;
use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    io::Write,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime},
};

use anyhow::Result;
use clap::Parser;
use dotenv::dotenv;
use ethers::providers::StreamExt;
use ethers::{
    abi::Token,
    prelude::*,
    types::transaction::eip712::{EIP712Domain, Eip712},
    utils::{keccak256, to_checksum},
};
use log::{debug, error, info};
use reqwest::Url;
use serde::{Deserialize, Serialize};

use crate::{
    config::{RewardConfig, ScheduledReward},
    contracts::LnRewardSystem,
    custom_serde::{checksumed_address, hex_bytes, u256_dec},
    graphql::{DebtEntry, ExchangeEntry, GraphqlClient, PerpFeeEntry, RewardClaim},
    wallet::{Wallet, WalletConfig},
    worker::{RewardComposition, WorkerClient},
};

mod config;
mod contracts;
mod custom_serde;
mod graphql;
mod wallet;
mod worker;

#[derive(Debug, Parser)]
#[clap(author, version, about)]
struct Cli {
    #[clap(long, env = "JSON_RPC", help = "URL of the JSON-RPC interface.")]
    json_rpc: Url,
    #[clap(long, env = "GRAPH_QUERY", help = "GraphQL query URL.")]
    graph_query: Url,
    #[clap(
        long,
        env = "LEGACY_CHAIN_JSON_RPC",
        help = "URL of the JSON-RPC interface of the legacy chain (optional)."
    )]
    legacy_chain_json_rpc: Option<Url>,

    #[clap(
        long,
        env = "REWARD_SYSTEM_ADDRESS",
        help = "Address of the LnRewardSystem contract."
    )]
    reward_system_address: Address,
    #[clap(
        long,
        env = "EIP_712_CONTRACT_NAME",
        default_value = "Linear",
        help = "Contract name for EIP-712 signatures."
    )]
    eip_712_contract_name: String,

    #[clap(flatten)]
    wallet: WalletConfig,

    #[clap(
        long,
        env = "PROCESS_INTERVAL",
        default_value = "6000",
        help = "The duration to pause between processing runs in milliseconds."
    )]
    process_interval: u64,
}

struct RunContext {
    chain_id: u64,
    signer: Wallet,
    eip_712_contract_name: String,
    reward_system_address: Address,
}

#[derive(PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RewardEntry {
    chain_id: u64,
    period_id: u32,
    #[serde(serialize_with = "checksumed_address::serialize")]
    recipient: Address,
    #[serde(with = "u256_dec")]
    staking_reward: U256,
    #[serde(with = "u256_dec")]
    fee_reward: U256,
}

#[derive(PartialEq, Eq, Serialize, Deserialize)]
struct TraceEntry {
    #[serde(serialize_with = "checksumed_address::serialize")]
    address: Address,
    #[serde(with = "u256_dec")]
    weight: U256,
}

#[derive(PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SignedRewardEntry {
    #[serde(flatten)]
    reward: RewardEntry,
    signatures: Vec<Signature>,
}

#[derive(PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Signature {
    #[serde(serialize_with = "checksumed_address::serialize")]
    signer: Address,
    #[serde(with = "hex_bytes")]
    signature: Vec<u8>,
}

impl PartialOrd for RewardEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.recipient.partial_cmp(&other.recipient)
    }
}

impl Ord for RewardEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.recipient.cmp(&other.recipient)
    }
}

impl PartialOrd for TraceEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.address.partial_cmp(&other.address)
    }
}

impl Ord for TraceEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.address.cmp(&other.address)
    }
}

trait PoolableFeeEntry {
    fn fee_for_pool(&self) -> U256;

    fn timestamp(&self) -> SystemTime;
}

impl PoolableFeeEntry for ExchangeEntry {
    fn fee_for_pool(&self) -> U256 {
        self.fee_for_pool
    }

    fn timestamp(&self) -> SystemTime {
        self.timestamp
    }
}

impl PoolableFeeEntry for PerpFeeEntry {
    fn fee_for_pool(&self) -> U256 {
        self.fee_for_pool
    }

    fn timestamp(&self) -> SystemTime {
        self.timestamp
    }
}

impl fmt::Debug for SignedRewardEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Implement how you want to format the struct for debugging
        // For example, print the important fields
        write!(f, "SignedRewardEntry {{ /* fields here */ }}")
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    env_logger::init();

    let cli = Cli::parse();

    debug!("Collecting settings from contract via JSON-RPC...");
    let rpc_provider = Arc::new(Provider::new(Http::new_with_client(
        cli.json_rpc.clone(),
        reqwest::ClientBuilder::new()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap(),
    )));
    let chain_id = rpc_provider.get_chainid().await?.as_u64();
    info!("Chain Id: {}", chain_id);

    let signer = Wallet::from_source(&cli.wallet, chain_id).await?;
    info!("Reward signer: {}", to_checksum(&signer.address(), None));

    info!(
        "Reward System: {}",
        to_checksum(&cli.reward_system_address, None)
    );

    let run_context = RunContext {
        chain_id,
        signer,
        eip_712_contract_name: cli.eip_712_contract_name,
        reward_system_address: cli.reward_system_address,
    };

    loop {
        if let Err(err) = run_once(&run_context).await {
            error!("Error: {err}");
        }

        std::thread::sleep(Duration::from_millis(cli.process_interval));
    }
}

async fn run_once(run_context: &RunContext) -> Result<()> {
    debug!("Signing rewards generated...");
    let mut reward_entries = vec![];
    let recipient_hex = "0x5C9d6aFE82C8f1c33aB274C577932F2D40778347";
    let recipient = Address::from_slice(&hex::decode(&recipient_hex[2..]).unwrap());
    reward_entries.push(RewardEntry {
        chain_id: run_context.chain_id,
        period_id: 136,
        recipient: recipient,
        staking_reward: U256::from_dec_str("10000000000000000000000").unwrap(),
        fee_reward: U256::from_dec_str("1000000000000000").unwrap(),
    });

    let signed_reward_entries = sign_rewards(
        reward_entries,
        &run_context.signer,
        run_context.chain_id,
        &run_context.eip_712_contract_name,
        run_context.reward_system_address,
    )
    .await?;
    for entry in &signed_reward_entries {
        info!("Sign Entry: {:?}", encode(&entry.signatures[0].signature));
    }
    info!("Finished signing rewards");

    Ok(())
}

async fn sign_rewards(
    reward_entries: Vec<RewardEntry>,
    signer: &Wallet,
    chain_id: u64,
    contract_name: &str,
    contract_address: Address,
) -> Result<Vec<SignedRewardEntry>> {
    struct Eip712RewardEntry<'a> {
        inner: &'a RewardEntry,
        chain_id: u64,
        contract_name: &'a str,
        contract_address: Address,
    }

    impl<'a> Eip712 for Eip712RewardEntry<'a> {
        type Error = std::convert::Infallible;

        fn domain(&self) -> std::result::Result<EIP712Domain, Self::Error> {
            Ok(EIP712Domain {
                name: Some(self.contract_name.to_owned()),
                version: Some("1".into()),
                chain_id: Some(self.chain_id.into()),
                verifying_contract: Some(self.contract_address),
                salt: None,
            })
        }

        fn type_hash() -> std::result::Result<[u8; 32], Self::Error> {
            Ok(keccak256(
                "Reward(uint256 periodId,address recipient,uint256 stakingReward,uint256 feeReward)",
            ))
        }

        fn struct_hash(&self) -> std::result::Result<[u8; 32], Self::Error> {
            Ok(keccak256(abi::encode(&[
                Token::Uint(U256::from(Self::type_hash()?)),
                Token::Uint(self.inner.period_id.into()),
                Token::Address(self.inner.recipient),
                Token::Uint(self.inner.staking_reward),
                Token::Uint(self.inner.fee_reward),
            ])))
        }
    }

    let mut signed_entries = vec![];

    for entry in reward_entries.into_iter() {
        let mut failed_attempts = 0;

        let signature = loop {
            match signer
                .sign_typed_data(&Eip712RewardEntry {
                    inner: &entry,
                    chain_id,
                    contract_name,
                    contract_address,
                })
                .await
            {
                Ok(value) => break value,
                Err(err) => {
                    failed_attempts += 1;
                    if failed_attempts >= 10 {
                        anyhow::bail!("Signing still fails after 10 attempts");
                    } else {
                        error!(
                            "Failed to sign reward entry. Retrying (attempt {}) after 10 seconds: {}",
                            failed_attempts + 1,
                            err
                        );
                        tokio::time::sleep(Duration::from_secs(10)).await;
                    }
                }
            }
        };

        signed_entries.push(SignedRewardEntry {
            reward: entry,
            signatures: vec![Signature {
                signer: signer.address(),
                signature: signature.to_vec(),
            }],
        })
    }

    Ok(signed_entries)
}

fn parse_sha256_sum(value: &str) -> Result<[u8; 32]> {
    let parsed_bytes = hex::decode(value.trim_start_matches("0x"))?;
    if parsed_bytes.len() != 32 {
        anyhow::bail!("invalid byte length: {}", parsed_bytes.len());
    }

    let mut buffer = [0u8; 32];
    buffer.copy_from_slice(&parsed_bytes);

    Ok(buffer)
}
