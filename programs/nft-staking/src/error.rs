use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Collection staking statistics are missing or invalid")]
    InvalidCollectionStats,
    #[msg("Reward arithmetic overflow")]
    RewardOverflow,
    #[msg("No rewards have accumulated yet")]
    NoRewardsAvailable,
    #[msg("The staking program must control the frozen asset and burn delegate")]
    InvalidStakingPlugins,
    #[msg("Only the collection creator may initialize its staking configuration")]
    InvalidAdmin,
    #[msg("Invalid owner")]
    InvalidOwner,
    #[msg("Invalid update authority")]
    InvalidUpdateAuthority,
    #[msg("Asset is already staked")]
    AssetStaked,
    #[msg("Asset is not staked")]
    AssetNotStaked,
    #[msg("Freeze period has not passed")]
    FreezePeriodNotOver,
    #[msg("Invalid freeze period")]
    InvalidFreezePeriod,
    #[msg("Invalid rewards bps")]
    InvalidRewardsBps,
    #[msg("Invalid timestamp")]
    InvalidTimeStamp,
}
