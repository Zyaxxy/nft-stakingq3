use super::{claim_rewards::StakedAsset, staking_helpers::*};
use crate::{constants::BURN_BONUS_TOKENS, error::ErrorCode};
use anchor_lang::prelude::*;
use mpl_core::instructions::BurnV1CpiBuilder;

pub fn handler(ctx: Context<StakedAsset>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require_lock_elapsed(&ctx.accounts.stake_position, &ctx.accounts.config, now)?;
    assert_staking_plugins(
        &ctx.accounts.asset.to_account_info(),
        ctx.accounts.update_authority.key(),
    )?;
    let (accrued, _) = calculate_rewards(
        &ctx.accounts.stake_position,
        &ctx.accounts.config,
        ctx.accounts.rewards_mint.decimals,
        now,
    )?;
    let bonus = BURN_BONUS_TOKENS
        .checked_mul(
            10u64
                .checked_pow(ctx.accounts.rewards_mint.decimals.into())
                .ok_or(ErrorCode::RewardOverflow)?,
        )
        .ok_or(ErrorCode::RewardOverflow)?;
    let amount = accrued
        .checked_add(bonus)
        .ok_or(ErrorCode::RewardOverflow)?;
    // BurnDelegate cannot bypass freezing. This thaw and burn are atomic; all
    // changes roll back if minting or collection accounting subsequently fails.
    thaw_asset(ctx.accounts, ctx.bumps.update_authority)?;
    let collection_key = ctx.accounts.collection.key();
    let seeds = [
        b"update_authority".as_ref(),
        collection_key.as_ref(),
        &[ctx.bumps.update_authority],
    ];
    BurnV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .payer(&ctx.accounts.owner.to_account_info())
        .authority(Some(&ctx.accounts.update_authority.to_account_info()))
        .system_program(Some(&ctx.accounts.system_program.to_account_info()))
        .invoke_signed(&[&seeds])?;
    mint_rewards(ctx.accounts, amount)?;
    decrement_total_staked(ctx.accounts, ctx.bumps.update_authority)?;
    // A burned Core asset and closed staking PDA cannot receive a second bonus.
    ctx.accounts
        .stake_position
        .close(ctx.accounts.owner.to_account_info())?;
    Ok(())
}
