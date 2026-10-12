use super::staking_helpers::{
    assert_staking_plugins, calculate_rewards, mint_rewards, require_lock_elapsed,
};
use crate::{
    core_accounts::{CoreAsset, CoreCollection},
    error::ErrorCode,
    Config, StakePosition,
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};
use mpl_core::types::UpdateAuthority;

/// Fully constrained accounts shared by claim, unstake, and burn.
#[derive(Accounts)]
pub struct StakedAsset<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,
    #[account(seeds = [b"config", collection.key().as_ref()], bump = config.bump)]
    pub config: Account<'info, Config>,
    #[account(mut, constraint = asset.owner == owner.key() @ ErrorCode::InvalidOwner,
        constraint = asset.update_authority == UpdateAuthority::Collection(collection.key()) @ ErrorCode::InvalidUpdateAuthority)]
    pub asset: Account<'info, CoreAsset>,
    #[account(mut, constraint = collection.update_authority == update_authority.key() @ ErrorCode::InvalidUpdateAuthority)]
    pub collection: Account<'info, CoreCollection>,
    /// CHECK: This collection's program-controlled signing PDA.
    #[account(seeds = [b"update_authority", collection.key().as_ref()], bump)]
    pub update_authority: UncheckedAccount<'info>,
    #[account(mut, seeds = [b"stake", asset.key().as_ref()], bump = stake_position.bump,
        has_one = owner @ ErrorCode::InvalidOwner, has_one = asset, has_one = collection)]
    pub stake_position: Account<'info, StakePosition>,
    #[account(mut, seeds = [b"rewards_mint", config.key().as_ref()], bump = config.rewards_bump,
        mint::token_program = token_program, mint::authority = config)]
    pub rewards_mint: InterfaceAccount<'info, Mint>,
    #[account(init_if_needed, payer = owner, associated_token::mint = rewards_mint,
        associated_token::authority = owner, associated_token::token_program = token_program)]
    pub user_rewards_ata: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    /// CHECK: Fixed executable Metaplex Core program.
    #[account(address = mpl_core::ID, executable)]
    pub mpl_core_program: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<StakedAsset>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require_lock_elapsed(&ctx.accounts.stake_position, &ctx.accounts.config, now)?;
    assert_staking_plugins(
        &ctx.accounts.asset.to_account_info(),
        ctx.accounts.update_authority.key(),
    )?;
    let (amount, remainder) = calculate_rewards(
        &ctx.accounts.stake_position,
        &ctx.accounts.config,
        ctx.accounts.rewards_mint.decimals,
        now,
    )?;
    require!(amount > 0, ErrorCode::NoRewardsAvailable);
    mint_rewards(ctx.accounts, amount)?;
    ctx.accounts.stake_position.last_claimed_at = now;
    ctx.accounts.stake_position.reward_remainder = remainder;
    // The NFT and delegate plugins are untouched: it stays staked and frozen.
    Ok(())
}
