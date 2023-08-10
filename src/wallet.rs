use anyhow::Result;
use clap::Parser;
use ethers::{
    prelude::*,
    types::transaction::{eip2718::TypedTransaction, eip712::Eip712},
};
use rusoto_core::{credential::ContainerProvider, Region};
use rusoto_kms::KmsClient;

#[derive(Debug)]
pub enum Wallet {
    LocalWallet(LocalWallet),
    Aws(AwsSigner),
}

#[derive(thiserror::Error, Debug)]
#[error(transparent)]
pub enum WalletError {
    LocalWallet(<LocalWallet as Signer>::Error),
    Aws(<AwsSigner as Signer>::Error),
}

#[derive(Debug, Parser)]
pub struct WalletConfig {
    #[clap(
        long,
        env = "PRIVATE_KEY",
        help = "Private key of the account in plain text. (Only use for development)"
    )]
    private_key: Option<LocalWallet>,
    #[clap(
        long,
        env = "AWS_KEY_ID",
        help = "Key ID for the AWS KMS key store. (Only use for production)"
    )]
    aws_key_id: Option<String>,
    #[clap(
        long,
        env = "AWS_REGION",
        help = "AWS region for the AWS KMS key store. (Only use for production)"
    )]
    aws_region: Option<Region>,
}

pub trait WalletSource {
    fn private_key(&self) -> &Option<LocalWallet>;

    fn aws_key_id(&self) -> &Option<String>;

    fn aws_region(&self) -> &Option<Region>;
}

impl Wallet {
    pub async fn from_source<S>(source: &S, chain_id: u64) -> Result<Self>
    where
        S: WalletSource,
    {
        Ok(match (source.private_key(), source.aws_key_id()) {
            (Some(private_key), None) => {
                Wallet::LocalWallet(private_key.clone()).with_chain_id(chain_id)
            }
            (None, Some(aws_key_id)) => {
                let aws_region = source
                    .aws_region()
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("AWS region not provided"))?;

                let kms_client = KmsClient::new_with_client(
                    rusoto_core::Client::new_with(
                        ContainerProvider::new(),
                        rusoto_core::HttpClient::new().unwrap(),
                    ),
                    aws_region,
                );

                Wallet::Aws(AwsSigner::new(kms_client, aws_key_id, chain_id).await?)
            }
            _ => anyhow::bail!("more than 1 key store provided"),
        })
    }
}

#[async_trait::async_trait]
impl Signer for Wallet {
    type Error = WalletError;

    async fn sign_message<S: Send + Sync + AsRef<[u8]>>(
        &self,
        message: S,
    ) -> Result<Signature, Self::Error> {
        match self {
            Self::LocalWallet(inner) => inner
                .sign_message(message)
                .await
                .map_err(Self::Error::LocalWallet),
            Self::Aws(inner) => inner.sign_message(message).await.map_err(Self::Error::Aws),
        }
    }

    async fn sign_transaction(&self, message: &TypedTransaction) -> Result<Signature, Self::Error> {
        match self {
            Self::LocalWallet(inner) => inner
                .sign_transaction(message)
                .await
                .map_err(Self::Error::LocalWallet),
            Self::Aws(inner) => inner
                .sign_transaction(message)
                .await
                .map_err(Self::Error::Aws),
        }
    }

    async fn sign_typed_data<T: Eip712 + Send + Sync>(
        &self,
        payload: &T,
    ) -> Result<Signature, Self::Error> {
        match self {
            Self::LocalWallet(inner) => inner
                .sign_typed_data(payload)
                .await
                .map_err(Self::Error::LocalWallet),
            Self::Aws(inner) => inner
                .sign_typed_data(payload)
                .await
                .map_err(Self::Error::Aws),
        }
    }

    fn address(&self) -> Address {
        match self {
            Self::LocalWallet(inner) => inner.address(),
            Self::Aws(inner) => inner.address(),
        }
    }

    fn chain_id(&self) -> u64 {
        match self {
            Self::LocalWallet(inner) => inner.chain_id(),
            Self::Aws(inner) => inner.chain_id(),
        }
    }

    fn with_chain_id<T: Into<u64>>(self, chain_id: T) -> Self {
        match self {
            Self::LocalWallet(inner) => Self::LocalWallet(inner.with_chain_id(chain_id)),
            Self::Aws(inner) => Self::Aws(inner.with_chain_id(chain_id)),
        }
    }
}

impl WalletSource for WalletConfig {
    fn private_key(&self) -> &Option<LocalWallet> {
        &self.private_key
    }

    fn aws_key_id(&self) -> &Option<String> {
        &self.aws_key_id
    }

    fn aws_region(&self) -> &Option<Region> {
        &self.aws_region
    }
}
