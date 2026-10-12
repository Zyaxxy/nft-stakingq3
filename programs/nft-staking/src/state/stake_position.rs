use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct StakePosition {
    pub owner: Pubkey,
    pub asset: Pubkey,
    pub collection: Pubkey,
    pub staked_at: i64,
    pub last_claimed_at: i64,
    // Numerator left over after converting elapsed seconds to reward base units.
    pub reward_remainder: u64,
    pub bump: u8,
}
