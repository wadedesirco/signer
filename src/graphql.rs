use std::time::{Duration, SystemTime};

use anyhow::Result;
use ethers::prelude::*;
use log::error;
use reqwest::{Client as HttpClient, Url};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub struct GraphqlClient {
    client: HttpClient,
    query_url: Url,
    anchor_block: u64,
}

pub struct DebtEntry {
    pub id: String,
    pub index: u64,
    pub address: Address,
    pub debt_factor: U256,
    pub debt_proportion: U256,
    pub timestamp: SystemTime,
}

pub struct ExchangeEntry {
    pub id: String,
    pub index: u64,
    pub from_addr: Address,
    pub source_key: String,
    pub source_amount: U256,
    pub dest_addr: Address,
    pub dest_key: String,
    pub dest_recived: U256,
    pub fee_for_pool: U256,
    pub fee_for_foundation: U256,
    pub timestamp: SystemTime,
}

pub struct PerpFeeEntry {
    pub id: String,
    pub index: u64,
    pub fee_for_pool: U256,
    pub fee_for_foundation: U256,
    pub timestamp: SystemTime,
}

pub struct RewardClaim {
    pub id: String,
    pub index: u64,
    pub recipient: Address,
    pub period_id: u32,
    pub staking_reward: U256,
    pub fee_reward: U256,
}

#[derive(Serialize, Deserialize)]
struct GraphQueryRequest {
    query: String,
    variables: GraphQueryVariables,
}

#[derive(Serialize, Deserialize)]
struct GraphQueryVariables {
    block: u64,
    first: usize,
    skip: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum GraphQueryResponse<D> {
    Success(GraphQuerySuccessResponse<D>),
    Error(GraphQueryErrorResponse),
}

#[derive(Serialize, Deserialize)]
struct GraphQuerySuccessResponse<D> {
    data: D,
}

#[derive(Serialize, Deserialize)]
struct GraphQueryErrorResponse {
    errors: Vec<GraphQueryError>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawQueryResponseData<R> {
    entries: Vec<R>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawDebtEntry {
    pub id: String,
    pub index: String,
    pub address: String,
    pub debt_factor: String,
    pub debt_proportion: String,
    pub timestamp: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawExchangeEntry {
    pub id: String,
    pub index: String,
    pub from_addr: String,
    pub source_key: String,
    pub source_amount: String,
    pub dest_addr: String,
    pub dest_key: String,
    pub dest_recived: String,
    pub fee_for_pool: String,
    pub fee_for_foundation: String,
    pub timestamp: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPerpFeeEntry {
    pub id: String,
    pub index: String,
    pub fee_for_pool: String,
    pub fee_for_foundation: String,
    pub timestamp: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRewardClaim {
    pub id: String,
    pub index: String,
    pub recipient: String,
    pub period_id: String,
    pub staking_reward: String,
    pub fee_reward: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraphQueryError {
    message: String,
}

// Hard-coded params
const QUERY_ENTRY_COUNT: usize = 1000;
const GRAPHQL_RETRY_COUNT: u32 = 5;

impl GraphqlClient {
    pub fn new(query_url: Url, anchor_block: u64, timeout: Duration) -> Self {
        Self {
            client: reqwest::ClientBuilder::new()
                .timeout(timeout)
                .build()
                .unwrap(),
            query_url,
            anchor_block,
        }
    }

    pub async fn get_debt_entries(&self) -> Result<Vec<DebtEntry>> {
        Self::get_entries_in_batches::<_, RawDebtEntry>(
            self,
            include_str!("./graphql/debt_entries_query.graphql"),
        )
        .await
    }

    pub async fn get_exchange_entries(&self) -> Result<Vec<ExchangeEntry>> {
        Self::get_entries_in_batches::<_, RawExchangeEntry>(
            self,
            include_str!("./graphql/exchange_entries_query.graphql"),
        )
        .await
    }

    pub async fn get_perp_fee_entries(&self) -> Result<Vec<PerpFeeEntry>> {
        Self::get_entries_in_batches::<_, RawPerpFeeEntry>(
            self,
            include_str!("./graphql/perp_fee_entries_query.graphql"),
        )
        .await
    }

    pub async fn get_reward_claims(&self) -> Result<Vec<RewardClaim>> {
        Self::get_entries_in_batches::<_, RawRewardClaim>(
            self,
            include_str!("./graphql/reward_claims_query.graphql"),
        )
        .await
    }

    async fn get_entries_in_batches<T, R>(&self, query_str: &str) -> Result<Vec<T>>
    where
        R: TryInto<T> + DeserializeOwned,
    {
        let mut entries = vec![];

        loop {
            let query = GraphQueryRequest {
                query: String::from(query_str),
                variables: GraphQueryVariables {
                    block: self.anchor_block,
                    first: QUERY_ENTRY_COUNT,
                    skip: entries.len(),
                },
            };

            let mut ind_retry = 0;
            let result = loop {
                match self.try_get_batch::<R>(&query).await {
                    Ok(value) => break value,
                    Err(err) => {
                        error!("GraphQL request attempt {} failed: {}", ind_retry, err);
                    }
                }

                ind_retry += 1;
                if ind_retry > GRAPHQL_RETRY_COUNT {
                    anyhow::bail!(
                        "GraphQL request still failed after {} retries",
                        GRAPHQL_RETRY_COUNT
                    );
                }
            };

            let batch_size = result.data.entries.len();

            entries.append(
                &mut result
                    .data
                    .entries
                    .into_iter()
                    .map(|item| {
                        item.try_into()
                            .map_err(|_| anyhow::anyhow!("error parsing raw result"))
                    })
                    .collect::<Result<Vec<_>>>()?,
            );

            if batch_size < QUERY_ENTRY_COUNT {
                break;
            }
        }

        Ok(entries)
    }

    async fn try_get_batch<R>(
        &self,
        request: &GraphQueryRequest,
    ) -> Result<GraphQuerySuccessResponse<RawQueryResponseData<R>>>
    where
        R: DeserializeOwned,
    {
        let res = self
            .client
            .post(self.query_url.clone())
            .json(&request)
            .send()
            .await?;

        match res.json().await? {
            GraphQueryResponse::Success(result) => Ok(result),
            GraphQueryResponse::Error(err) => Err(anyhow::anyhow!("error: {:?}", err.errors)),
        }
    }
}

impl TryFrom<RawDebtEntry> for DebtEntry {
    type Error = anyhow::Error;

    fn try_from(value: RawDebtEntry) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            index: value.index.parse()?,
            address: value.address.parse()?,
            debt_factor: U256::from_dec_str(&value.debt_factor)?,
            debt_proportion: U256::from_dec_str(&value.debt_proportion)?,
            timestamp: SystemTime::UNIX_EPOCH + Duration::from_secs(value.timestamp.parse()?),
        })
    }
}

impl TryFrom<RawExchangeEntry> for ExchangeEntry {
    type Error = anyhow::Error;

    fn try_from(value: RawExchangeEntry) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            index: value.index.parse()?,
            from_addr: value.from_addr.parse()?,
            source_key: value.source_key,
            source_amount: U256::from_dec_str(&value.source_amount)?,
            dest_addr: value.dest_addr.parse()?,
            dest_key: value.dest_key,
            dest_recived: U256::from_dec_str(&value.dest_recived)?,
            fee_for_pool: U256::from_dec_str(&value.fee_for_pool)?,
            fee_for_foundation: U256::from_dec_str(&value.fee_for_foundation)?,
            timestamp: SystemTime::UNIX_EPOCH + Duration::from_secs(value.timestamp.parse()?),
        })
    }
}

impl TryFrom<RawPerpFeeEntry> for PerpFeeEntry {
    type Error = anyhow::Error;

    fn try_from(value: RawPerpFeeEntry) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            index: value.index.parse()?,
            fee_for_pool: U256::from_dec_str(&value.fee_for_pool)?,
            fee_for_foundation: U256::from_dec_str(&value.fee_for_foundation)?,
            timestamp: SystemTime::UNIX_EPOCH + Duration::from_secs(value.timestamp.parse()?),
        })
    }
}

impl TryFrom<RawRewardClaim> for RewardClaim {
    type Error = anyhow::Error;

    fn try_from(value: RawRewardClaim) -> std::result::Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            index: value.index.parse()?,
            recipient: value.recipient.parse()?,
            period_id: value.period_id.parse()?,
            staking_reward: U256::from_dec_str(&value.staking_reward)?,
            fee_reward: U256::from_dec_str(&value.fee_reward)?,
        })
    }
}
