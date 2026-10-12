#![allow(unexpected_cfgs, deprecated, ambiguous_glob_reexports)]

pub mod constants;
pub mod core_accounts;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("7u8ZNXw87H75SW6fpMPfGFN8n1G6qxoigLFp9BbMNFsb");

#[program]
pub mod nft_staking {

    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        rewards_bps: u16,
        freeze_period: u16,
    ) -> Result<()> {
        initialize::handler(ctx, rewards_bps, freeze_period)
    }

    pub fn create_collection(
        ctx: Context<CreateCollection>,
        name: String,
        uri: String,
    ) -> Result<()> {
        create_collection::handler(ctx, name, uri)
    }

    pub fn mint_asset(ctx: Context<MintAsset>, name: String, uri: String) -> Result<()> {
        mint_asset::handler(ctx, name, uri)
    }

    pub fn stake(ctx: Context<Stake>) -> Result<()> {
        stake::handler(ctx)
    }

    pub fn unstake(ctx: Context<StakedAsset>) -> Result<()> {
        unstake::handler(ctx)
    }

    pub fn claim_rewards(ctx: Context<StakedAsset>) -> Result<()> {
        claim_rewards::handler(ctx)
    }

    pub fn burn_staked_nft(ctx: Context<StakedAsset>) -> Result<()> {
        burn_staked_nft::handler(ctx)
    }
}
