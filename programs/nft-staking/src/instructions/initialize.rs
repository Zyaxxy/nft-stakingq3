use crate::constants::STAKING_ADMIN;
use crate::core_accounts::CoreCollection;
use crate::error::ErrorCode;
use crate::state::Config;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenInterface};
use mpl_core::{
    accounts::BaseCollectionV1,
    fetch_plugin,
    types::{Attributes, PluginType},
};

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init,
        payer = admin,
        space = Config::DISCRIMINATOR.len() + Config::INIT_SPACE,
        seeds = [b"config", collection.key().as_ref()],
        bump,
    )]
    pub config: Account<'info, Config>,
    #[account( constraint = collection.update_authority == update_authority.key() @ ErrorCode::InvalidUpdateAuthority )]
    pub collection: Account<'info, CoreCollection>,
    ///CHECK: This account is not initialized and it is used for signing purposes only, we verify that derives from the correct seeds
    #[account(
        seeds = [b"update_authority", collection.key().as_ref()],
        bump,
    )]
    pub update_authority: UncheckedAccount<'info>,
    #[account(
        init,
        payer = admin,
        mint::decimals = 6,
        mint::authority = config,
        mint::token_program = token_program,
        seeds = [b"rewards_mint", config.key().as_ref()],
        bump,
    )]
    pub rewards_mint: InterfaceAccount<'info, Mint>,
    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handler(ctx: Context<Initialize>, rewards_bps: u16, freeze_period: u16) -> Result<()> {
    let (_, attributes, _) = fetch_plugin::<BaseCollectionV1, Attributes>(
        &ctx.accounts.collection.to_account_info(),
        PluginType::Attributes,
    )
    .map_err(|_| ErrorCode::InvalidAdmin)?;
    require!(
        attributes
            .attribute_list
            .iter()
            .any(|attribute| attribute.key == STAKING_ADMIN
                && attribute.value == ctx.accounts.admin.key().to_string()),
        ErrorCode::InvalidAdmin
    );
    require!(
        rewards_bps > 0 && rewards_bps <= 10_000,
        ErrorCode::InvalidRewardsBps
    );
    ctx.accounts.config.set_inner(Config {
        rewards_bps,
        freeze_period,
        rewards_bump: ctx.bumps.rewards_mint,
        bump: ctx.bumps.config,
    });
    Ok(())
}
