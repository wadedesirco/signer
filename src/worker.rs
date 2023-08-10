use std::time::Duration;

use anyhow::Result;
use ethers::{prelude::*, utils::to_checksum};
use log::debug;
use reqwest::{Client as HttpClient, Url};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sha2::Digest;

use crate::{
    config::RewardConfig,
    custom_serde::{checksumed_address, hex_bytes, u256_dec, ChecksumedAddress},
};

pub struct WorkerClient {
    client: HttpClient,
    base_url: Url,
    admin_token: String,
}

#[serde_as]
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerConfig {
    pub first_period_start_time: u64,
    pub period_duration: u64,
    #[serde_as(serialize_as = "Vec<ChecksumedAddress>")]
    pub signers: Vec<Address>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Submission {
    pub period_id: u32,
    pub chain_id: u64,
    #[serde(serialize_with = "checksumed_address::serialize")]
    pub signer: Address,
    pub entries: Vec<SubmissionRewardEntry>,
    pub composition: RewardComposition,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SubmissionRewardEntry {
    #[serde(serialize_with = "checksumed_address::serialize")]
    pub recipient: Address,
    #[serde(with = "u256_dec")]
    pub staking_reward: U256,
    #[serde(with = "u256_dec")]
    pub fee_reward: U256,
    #[serde(with = "hex_bytes")]
    pub signature: Vec<u8>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RewardComposition {
    #[serde(with = "u256_dec")]
    pub scheduled_staking_rewards: U256,
    #[serde(with = "u256_dec")]
    pub rollover_staking_rewards: U256,
    #[serde(with = "u256_dec")]
    pub fees_accumulated: U256,
    #[serde(with = "u256_dec")]
    pub rollover_fees: U256,
}

impl WorkerClient {
    pub fn new(base_url: Url, admin_token: String, timeout: Duration) -> Self {
        Self {
            client: reqwest::ClientBuilder::new()
                .timeout(timeout)
                .build()
                .unwrap(),
            base_url,
            admin_token,
        }
    }

    pub async fn get_worker_config(&self) -> Result<Option<WorkerConfig>> {
        let response = self
            .client
            .get(format!("{}admin/workerConfig", self.base_url))
            .header("Authorization", format!("Bearer {}", self.admin_token))
            .send()
            .await?;

        let status_code = response.status();
        if !status_code.is_success() {
            let response_text = response.text().await?;
            debug!("Unsuccessful repsonse text: {}", response_text);

            anyhow::bail!("unsuccessful status code: {}", status_code);
        } else {
            Ok(response.json().await?)
        }
    }

    pub async fn get_reward_config_checked(&self, checksum: &[u8; 32]) -> Result<RewardConfig> {
        let response = self
            .client
            .get(format!("{}admin/rewardConfig", self.base_url))
            .header("Authorization", format!("Bearer {}", self.admin_token))
            .send()
            .await?;

        let status_code = response.status();
        if !status_code.is_success() {
            let response_text = response.text().await?;
            debug!("Unsuccessful repsonse text: {}", response_text);

            anyhow::bail!("unsuccessful status code: {}", status_code);
        } else {
            let raw_text = response.text().await?;

            let mut hasher = sha2::Sha256::default();
            hasher.update(raw_text.as_bytes());

            let hash_from_worker = hasher.finalize();
            let hash_from_worker = hash_from_worker.as_slice();

            if !checksum.eq(hash_from_worker) {
                anyhow::bail!(
                    "config checksum mismatch: expected: {}; actual: {}",
                    hex::encode(checksum),
                    hex::encode(hash_from_worker)
                );
            }

            Ok(serde_json::from_str(&raw_text)?)
        }
    }

    pub async fn get_last_period_id(&self) -> Result<u32> {
        let response = self
            .client
            .get(format!("{}lastPeriodId", self.base_url))
            .header("Authorization", format!("Bearer {}", self.admin_token))
            .send()
            .await?;

        let status_code = response.status();
        if !status_code.is_success() {
            let response_text = response.text().await?;
            debug!("Unsuccessful repsonse text: {}", response_text);

            anyhow::bail!("unsuccessful status code: {}", status_code);
        } else {
            Ok(response.json().await?)
        }
    }

    pub async fn get_signer_staged(&self, period_id: u32, signer: &Address) -> Result<bool> {
        let response = self
            .client
            .get(format!(
                "{}admin/signerStaged?periodId={}&signer={}",
                self.base_url,
                period_id,
                to_checksum(signer, None)
            ))
            .header("Authorization", format!("Bearer {}", self.admin_token))
            .send()
            .await?;

        let status_code = response.status();
        if !status_code.is_success() {
            let response_text = response.text().await?;
            debug!("Unsuccessful repsonse text: {}", response_text);

            anyhow::bail!("unsuccessful status code: {}", status_code);
        } else {
            Ok(response.json().await?)
        }
    }

    pub async fn get_stage_ready(&self, period_id: u32) -> Result<bool> {
        let response = self
            .client
            .get(format!(
                "{}admin/stageReady?periodId={}",
                self.base_url, period_id
            ))
            .header("Authorization", format!("Bearer {}", self.admin_token))
            .send()
            .await?;

        let status_code = response.status();
        if !status_code.is_success() {
            let response_text = response.text().await?;
            debug!("Unsuccessful repsonse text: {}", response_text);

            anyhow::bail!("unsuccessful status code: {}", status_code);
        } else {
            Ok(response.json().await?)
        }
    }

    pub async fn set_worker_config(&self, config: &WorkerConfig) -> Result<()> {
        let response = self
            .client
            .post(format!("{}admin/workerConfig", self.base_url))
            .header("Authorization", format!("Bearer {}", self.admin_token))
            .json(&config)
            .send()
            .await?;

        let status_code = response.status();
        if !status_code.is_success() {
            let response_text = response.text().await?;
            debug!("Unsuccessful repsonse text: {}", response_text);

            anyhow::bail!("unsuccessful status code: {}", status_code);
        } else {
            Ok(())
        }
    }

    pub async fn stage(&self, submission: &Submission) -> Result<()> {
        let response = self
            .client
            .post(format!("{}admin/stage", self.base_url))
            .header("Authorization", format!("Bearer {}", self.admin_token))
            .json(&submission)
            .send()
            .await?;

        let status_code = response.status();
        if !status_code.is_success() {
            let response_text = response.text().await?;
            debug!("Unsuccessful repsonse text: {}", response_text);

            anyhow::bail!("unsuccessful status code: {}", status_code);
        } else {
            Ok(())
        }
    }

    pub async fn publish(&self, period_id: u32) -> Result<()> {
        let response = self
            .client
            .post(format!(
                "{}admin/publish?periodId={}",
                self.base_url, period_id
            ))
            .header("Authorization", format!("Bearer {}", self.admin_token))
            .send()
            .await?;

        let status_code = response.status();
        if !status_code.is_success() {
            let response_text = response.text().await?;
            debug!("Unsuccessful repsonse text: {}", response_text);

            anyhow::bail!("unsuccessful status code: {}", status_code);
        } else {
            Ok(())
        }
    }
}

impl RewardComposition {
    pub fn staking_reward_for_period(&self) -> U256 {
        self.scheduled_staking_rewards
            .checked_add(self.rollover_staking_rewards)
            .expect("overflow")
    }

    pub fn fee_reward_for_period(&self) -> U256 {
        self.fees_accumulated
            .checked_add(self.rollover_fees)
            .expect("overflow")
    }
}
