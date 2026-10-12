use crate::core_accounts::CoreCollection;
use crate::{constants::STAKING_ADMIN, error::ErrorCode};
use anchor_lang::prelude::*;
use mpl_core::{
    accounts::BaseCollectionV1, instructions::CreateV2CpiBuilder, ID as MPL_CORE_PROGRAM_ID,
};
use mpl_core::{
    fetch_plugin,
    types::{Attributes, PluginType},
};

#[derive(Accounts)]
pub struct MintAsset<'info> {
    #[account(mut)]
    pub user: Signer<'info>,
    #[account(mut)]
    pub asset: Signer<'info>,
    #[account(mut, constraint = collection.update_authority == update_authority.key() @ ErrorCode::InvalidUpdateAuthority)]
    pub collection: Account<'info, CoreCollection>,
    ///CHECK: This account is not initialized and it is used for signing purposes only, we verify that derives from the correct seeds
    #[account(
        seeds = [b"update_authority".as_ref(), collection.key().as_ref()],
        bump,
    )]
    pub update_authority: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
    ///CHECK: This is the ID of the MPL core program
    #[account(address = MPL_CORE_PROGRAM_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,
}

pub fn handler(ctx: Context<MintAsset>, name: String, uri: String) -> Result<()> {
    // This demo issues assets to its collection creator. Restricting issuance
    // prevents permissionless minting from becoming an unlimited burn faucet.
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
                && attribute.value == ctx.accounts.user.key().to_string()),
        ErrorCode::InvalidAdmin
    );
    let collection_key = ctx.accounts.collection.key();
    let signer_seeds = &[
        b"update_authority".as_ref(),
        collection_key.as_ref(),
        &[ctx.bumps.update_authority],
    ];

    CreateV2CpiBuilder::new(&ctx.accounts.mpl_core_program.to_account_info())
        .asset(&ctx.accounts.asset.to_account_info())
        .collection(Some(&ctx.accounts.collection.to_account_info()))
        .authority(Some(&ctx.accounts.update_authority.to_account_info()))
        .payer(&ctx.accounts.user.to_account_info())
        .owner(Some(&ctx.accounts.user.to_account_info()))
        .update_authority(None)
        .system_program(&ctx.accounts.system_program.to_account_info())
        .name(name)
        .uri(uri)
        .invoke_signed(&[signer_seeds])?;

    Ok(())
}
