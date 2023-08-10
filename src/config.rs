use ethers::prelude::*;
use serde::Deserialize;

use crate::custom_serde::u256_dec;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RewardConfig {
    pub has_legacy_chain: bool,
    pub exclude_list: Vec<Address>,
    pub staking_reward_schedule: Vec<ScheduledReward>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduledReward {
    pub period_id: u32,
    #[serde(with = "u256_dec")]
    pub reward: U256,
}
