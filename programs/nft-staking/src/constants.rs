use anchor_lang::prelude::*;

#[constant]
pub const SEED: &str = "anchor";
pub const SECOND_PER_DAY: i64 = 86400;
pub const TOTAL_STAKED: &str = "total_staked";
pub const STAKING_ADMIN: &str = "staking_admin";
pub const BURN_BONUS_TOKENS: u64 = 1_000;
