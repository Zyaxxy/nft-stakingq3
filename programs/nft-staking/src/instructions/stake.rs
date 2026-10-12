use super::staking_helpers::update_total_staked;
use crate::{
    core_accounts::{CoreAsset, CoreCollection},
    error::ErrorCode,
    Config, StakePosition,
};
use anchor_lang::prelude::*;
use mpl_core::{
    instructions::AddPluginV1CpiBuilder,
    types::{BurnDelegate, FreezeDelegate, Plugin, PluginAuthority, UpdateAuthority},
};

#[derive(Accounts)]
pub struct Stake<'info> {
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
    #[account(init, payer = owner, space = StakePosition::DISCRIMINATOR.len() + StakePosition::INIT_SPACE,
        seeds = [b"stake", asset.key().as_ref()], bump)]
    pub stake_position: Account<'info, StakePosition>,
    pub system_program: Program<'info, System>,
    /// CHECK: Fixed executable Metaplex Core program.
    #[account(address = mpl_core::ID, executable)]
    pub mpl_core_program: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<Stake>) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    ctx.accounts.stake_position.set_inner(StakePosition {
        owner: ctx.accounts.owner.key(),
        asset: ctx.accounts.asset.key(),
        collection: ctx.accounts.collection.key(),
        staked_at: now,
        last_claimed_at: now,
        reward_remainder: 0,
        bump: ctx.bumps.stake_position,
    });
    // The owner consents to both delegates. Add burn delegation before freezing.
    for plugin in [
        Plugin::BurnDelegate(BurnDelegate {}),
        Plugin::FreezeDelegate(FreezeDelegate { frozen: true }),
    ] {
        AddPluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
            .asset(&ctx.accounts.asset.to_account_info())
            .collection(Some(&ctx.accounts.collection.to_account_info()))
            .authority(Some(&ctx.accounts.owner.to_account_info()))
            .payer(&ctx.accounts.owner.to_account_info())
            .plugin(plugin)
            .init_authority(PluginAuthority::Address {
                address: ctx.accounts.update_authority.key(),
            })
            .system_program(&ctx.accounts.system_program.to_account_info())
            .invoke()?;
    }
    let collection_key = ctx.accounts.collection.key();
    let seeds = [
        b"update_authority".as_ref(),
        collection_key.as_ref(),
        &[ctx.bumps.update_authority],
    ];
    update_total_staked(
        &ctx.accounts.collection.to_account_info(),
        &ctx.accounts.update_authority.to_account_info(),
        &ctx.accounts.owner.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
        &ctx.accounts.mpl_core_program.to_account_info(),
        true,
        &seeds,
    )?;
    Ok(())
}
