use super::claim_rewards::StakedAsset;
use crate::{
    constants::{SECOND_PER_DAY, TOTAL_STAKED},
    error::ErrorCode,
    Config, StakePosition,
};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{mint_to_checked, MintToChecked};
use mpl_core::{
    accounts::{BaseAssetV1, BaseCollectionV1},
    fetch_plugin,
    instructions::{UpdateCollectionPluginV1CpiBuilder, UpdatePluginV1CpiBuilder},
    types::{Attributes, BurnDelegate, FreezeDelegate, Plugin, PluginAuthority, PluginType},
};

pub fn require_lock_elapsed(position: &StakePosition, config: &Config, now: i64) -> Result<()> {
    let elapsed = now
        .checked_sub(position.staked_at)
        .ok_or(ErrorCode::InvalidTimeStamp)?;
    require!(elapsed >= 0, ErrorCode::InvalidTimeStamp);
    require!(
        elapsed >= i64::from(config.freeze_period) * SECOND_PER_DAY,
        ErrorCode::FreezePeriodNotOver
    );
    Ok(())
}

pub fn calculate_rewards(
    position: &StakePosition,
    config: &Config,
    decimals: u8,
    now: i64,
) -> Result<(u64, u64)> {
    let elapsed = now
        .checked_sub(position.last_claimed_at)
        .ok_or(ErrorCode::InvalidTimeStamp)?;
    require!(elapsed >= 0, ErrorCode::InvalidTimeStamp);
    let denominator = SECOND_PER_DAY as u128 * 10_000;
    let scale = 10u128
        .checked_pow(decimals.into())
        .ok_or(ErrorCode::RewardOverflow)?;
    let numerator = (elapsed as u128)
        .checked_mul(config.rewards_bps.into())
        .and_then(|n| n.checked_mul(scale))
        .and_then(|n| n.checked_add(position.reward_remainder.into()))
        .ok_or(ErrorCode::RewardOverflow)?;
    let amount = u64::try_from(numerator / denominator).map_err(|_| ErrorCode::RewardOverflow)?;
    Ok((amount, (numerator % denominator) as u64))
}

pub fn assert_staking_plugins(asset: &AccountInfo, authority: Pubkey) -> Result<()> {
    let (freeze_authority, freeze, _) =
        fetch_plugin::<BaseAssetV1, FreezeDelegate>(asset, PluginType::FreezeDelegate)
            .map_err(|_| ErrorCode::InvalidStakingPlugins)?;
    let (burn_authority, _, _) =
        fetch_plugin::<BaseAssetV1, BurnDelegate>(asset, PluginType::BurnDelegate)
            .map_err(|_| ErrorCode::InvalidStakingPlugins)?;
    let expected = PluginAuthority::Address { address: authority };
    require!(
        freeze.frozen && freeze_authority == expected && burn_authority == expected,
        ErrorCode::InvalidStakingPlugins
    );
    Ok(())
}

pub fn mint_rewards(accounts: &StakedAsset, amount: u64) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    let collection_key = accounts.collection.key();
    let seeds = [
        b"config".as_ref(),
        collection_key.as_ref(),
        &[accounts.config.bump],
    ];
    mint_to_checked(
        CpiContext::new_with_signer(
            accounts.token_program.key(),
            MintToChecked {
                mint: accounts.rewards_mint.to_account_info(),
                to: accounts.user_rewards_ata.to_account_info(),
                authority: accounts.config.to_account_info(),
            },
            &[&seeds],
        ),
        amount,
        accounts.rewards_mint.decimals,
    )
}

pub fn thaw_asset(accounts: &StakedAsset, bump: u8) -> Result<()> {
    let collection_key = accounts.collection.key();
    let seeds = [
        b"update_authority".as_ref(),
        collection_key.as_ref(),
        &[bump],
    ];
    UpdatePluginV1CpiBuilder::new(&accounts.mpl_core_program.to_account_info())
        .asset(&accounts.asset.to_account_info())
        .collection(Some(&accounts.collection.to_account_info()))
        .authority(Some(&accounts.update_authority.to_account_info()))
        .payer(&accounts.owner.to_account_info())
        .plugin(Plugin::FreezeDelegate(FreezeDelegate { frozen: false }))
        .system_program(&accounts.system_program.to_account_info())
        .invoke_signed(&[&seeds])?;
    Ok(())
}

pub fn decrement_total_staked(accounts: &StakedAsset, bump: u8) -> Result<()> {
    let collection_key = accounts.collection.key();
    let seeds = [
        b"update_authority".as_ref(),
        collection_key.as_ref(),
        &[bump],
    ];
    update_total_staked(
        &accounts.collection.to_account_info(),
        &accounts.update_authority.to_account_info(),
        &accounts.owner.to_account_info(),
        &accounts.system_program.to_account_info(),
        &accounts.mpl_core_program.to_account_info(),
        false,
        &seeds,
    )
}

pub fn update_total_staked<'info>(
    collection: &AccountInfo<'info>,
    authority: &AccountInfo<'info>,
    payer: &AccountInfo<'info>,
    system: &AccountInfo<'info>,
    core: &AccountInfo<'info>,
    increment: bool,
    seeds: &[&[u8]],
) -> Result<()> {
    let (_, mut attributes, _) =
        fetch_plugin::<BaseCollectionV1, Attributes>(collection, PluginType::Attributes)
            .map_err(|_| ErrorCode::InvalidCollectionStats)?;
    let count = attributes
        .attribute_list
        .iter()
        .filter(|attribute| attribute.key == TOTAL_STAKED)
        .count();
    require!(count == 1, ErrorCode::InvalidCollectionStats);
    let attribute = attributes
        .attribute_list
        .iter_mut()
        .find(|attribute| attribute.key == TOTAL_STAKED)
        .ok_or(ErrorCode::InvalidCollectionStats)?;
    let current = attribute
        .value
        .parse::<u64>()
        .map_err(|_| ErrorCode::InvalidCollectionStats)?;
    let next = if increment {
        current.checked_add(1)
    } else {
        current.checked_sub(1)
    }
    .ok_or(ErrorCode::InvalidCollectionStats)?;
    attribute.value = next.to_string();
    UpdateCollectionPluginV1CpiBuilder::new(core)
        .collection(collection)
        .authority(Some(authority))
        .payer(payer)
        .plugin(Plugin::Attributes(attributes))
        .system_program(system)
        .invoke_signed(&[seeds])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn position() -> StakePosition {
        StakePosition {
            owner: Pubkey::default(),
            asset: Pubkey::default(),
            collection: Pubkey::default(),
            staked_at: 0,
            last_claimed_at: 0,
            reward_remainder: 0,
            bump: 0,
        }
    }
    fn config() -> Config {
        Config {
            rewards_bps: 10_000,
            freeze_period: 7,
            rewards_bump: 0,
            bump: 0,
        }
    }

    #[test]
    fn rewards_accrue_and_claims_do_not_reset_the_lock() {
        let mut position = position();
        let config = config();
        let now = 8 * SECOND_PER_DAY;
        assert_eq!(
            calculate_rewards(&position, &config, 6, now).unwrap(),
            (8_000_000, 0)
        );
        position.last_claimed_at = now;
        assert!(require_lock_elapsed(&position, &config, now).is_ok());
        assert_eq!(
            calculate_rewards(&position, &config, 6, now).unwrap(),
            (0, 0)
        );
        assert_eq!(
            calculate_rewards(&position, &config, 6, now + SECOND_PER_DAY).unwrap(),
            (1_000_000, 0)
        );
    }

    #[test]
    fn fractional_rewards_survive_many_claims() {
        let mut position = position();
        let mut config = config();
        config.rewards_bps = 123;
        let expected = calculate_rewards(&position, &config, 6, SECOND_PER_DAY).unwrap();
        let mut paid = 0;
        for now in 1..=SECOND_PER_DAY {
            let (amount, remainder) = calculate_rewards(&position, &config, 6, now).unwrap();
            paid += amount;
            position.last_claimed_at = now;
            position.reward_remainder = remainder;
        }
        assert_eq!((paid, position.reward_remainder), expected);
    }

    #[test]
    fn lock_and_arithmetic_reject_invalid_inputs() {
        let position = position();
        assert!(require_lock_elapsed(&position, &config(), 7 * SECOND_PER_DAY - 1).is_err());
        assert!(require_lock_elapsed(&position, &config(), 7 * SECOND_PER_DAY).is_ok());
        assert!(calculate_rewards(&position, &config(), 6, -1).is_err());
        assert!(calculate_rewards(&position, &config(), 38, i64::MAX).is_err());
        assert!(calculate_rewards(&position, &config(), 255, 1).is_err());
    }
}
