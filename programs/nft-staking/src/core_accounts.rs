//! Core's optional Anchor feature targets older Anchor releases. These small
//! read-only adapters use its default Borsh v1/Solana v3 SDK with Anchor v1.
use anchor_lang::prelude::*;
use mpl_core::{
    accounts::{BaseAssetV1, BaseCollectionV1},
    types::Key,
};

macro_rules! core_account {
    ($name:ident, $base:ty, $key:ident) => {
        #[derive(Clone)]
        pub struct $name(pub $base);

        impl std::ops::Deref for $name {
            type Target = $base;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
        impl AccountDeserialize for $name {
            fn try_deserialize_unchecked(buf: &mut &[u8]) -> Result<Self> {
                let account = <$base>::from_bytes(buf)
                    .map_err(|_| error!(anchor_lang::error::ErrorCode::AccountDidNotDeserialize))?;
                require!(
                    account.key == Key::$key,
                    anchor_lang::error::ErrorCode::AccountDiscriminatorMismatch
                );
                Ok(Self(account))
            }
        }
        // Core owns these accounts; only Core CPIs may write their data.
        impl AccountSerialize for $name {}
        impl Owner for $name {
            fn owner() -> Pubkey {
                mpl_core::ID
            }
        }
        impl Discriminator for $name {
            const DISCRIMINATOR: &'static [u8] = &[Key::$key as u8];
        }
        #[cfg(feature = "idl-build")]
        impl anchor_lang::IdlBuild for $name {}
    };
}
core_account!(CoreAsset, BaseAssetV1, AssetV1);
core_account!(CoreCollection, BaseCollectionV1, CollectionV1);
