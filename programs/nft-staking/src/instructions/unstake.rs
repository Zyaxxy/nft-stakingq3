use super::{claim_rewards::StakedAsset, staking_helpers::*};
use anchor_lang::prelude::*;
use mpl_core::{instructions::RemovePluginV1CpiBuilder, types::PluginType};

pub fn handler(ctx: Context<StakedAsset>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require_lock_elapsed(&ctx.accounts.stake_position, &ctx.accounts.config, now)?;
    assert_staking_plugins(
        &ctx.accounts.asset.to_account_info(),
        ctx.accounts.update_authority.key(),
    )?;
    let (amount, _) = calculate_rewards(
        &ctx.accounts.stake_position,
        &ctx.accounts.config,
        ctx.accounts.rewards_mint.decimals,
        now,
    )?;
    mint_rewards(ctx.accounts, amount)?;
    thaw_asset(ctx.accounts, ctx.bumps.update_authority)?;
    // Remove the owner-managed delegates so this NFT can be transferred/restaked.
    for plugin_type in [PluginType::FreezeDelegate, PluginType::BurnDelegate] {
        RemovePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
            .asset(&ctx.accounts.asset.to_account_info())
            .collection(Some(&ctx.accounts.collection.to_account_info()))
            .authority(Some(&ctx.accounts.owner.to_account_info()))
            .payer(&ctx.accounts.owner.to_account_info())
            .plugin_type(plugin_type)
            .system_program(&ctx.accounts.system_program.to_account_info())
            .invoke()?;
    }
    decrement_total_staked(ctx.accounts, ctx.bumps.update_authority)?;
    ctx.accounts
        .stake_position
        .close(ctx.accounts.owner.to_account_info())?;
    Ok(())
}
