use crate::state::IroError;
use anchor_lang::{prelude::*, solana_program::program::invoke_signed};
use mpl_core::{
    accounts::BaseAssetV1,
    instructions::{
        BurnV1, BurnV1InstructionArgs, CreateCollectionV2, CreateCollectionV2InstructionArgs,
        CreateV2, CreateV2InstructionArgs,
    },
    types::{DataState, Key, UpdateAuthority},
    ID,
};
pub const CORE_ID: Pubkey = ID;

pub fn collection<'info>(
    collection: AccountInfo<'info>,
    authority: AccountInfo<'info>,
    payer: AccountInfo<'info>,
    system: AccountInfo<'info>,
    program: AccountInfo<'info>,
    name: String,
    uri: String,
    seeds: &[&[&[u8]]],
) -> Result<()> {
    let ix = CreateCollectionV2 {
        collection: *collection.key,
        update_authority: Some(*authority.key),
        payer: *payer.key,
        system_program: *system.key,
    }
    .instruction(CreateCollectionV2InstructionArgs {
        name,
        uri,
        plugins: None,
        external_plugin_adapters: None,
    });
    invoke_signed(&ix, &[collection, authority, payer, system, program], seeds).map_err(Into::into)
}
pub fn claim<'info>(
    asset: AccountInfo<'info>,
    collection: AccountInfo<'info>,
    authority: AccountInfo<'info>,
    payer: AccountInfo<'info>,
    system: AccountInfo<'info>,
    program: AccountInfo<'info>,
    name: String,
    uri: String,
    seeds: &[&[&[u8]]],
) -> Result<()> {
    let ix = CreateV2 {
        asset: *asset.key,
        collection: Some(*collection.key),
        authority: Some(*authority.key),
        payer: *payer.key,
        owner: Some(*payer.key),
        update_authority: None,
        system_program: *system.key,
        log_wrapper: None,
    }
    .instruction(CreateV2InstructionArgs {
        data_state: DataState::AccountState,
        name,
        uri,
        plugins: None,
        external_plugin_adapters: None,
    });
    invoke_signed(
        &ix,
        &[asset, collection, authority, payer, system, program],
        seeds,
    )
    .map_err(Into::into)
}
pub fn burn<'info>(
    asset: AccountInfo<'info>,
    collection: AccountInfo<'info>,
    holder: AccountInfo<'info>,
    system: AccountInfo<'info>,
    program: AccountInfo<'info>,
) -> Result<()> {
    require_keys_eq!(*asset.owner, CORE_ID, IroError::Holder);
    let a =
        BaseAssetV1::from_bytes(&asset.try_borrow_data()?).map_err(|_| error!(IroError::Holder))?;
    require!(
        a.key == Key::AssetV1
            && a.owner == *holder.key
            && a.update_authority == UpdateAuthority::Collection(*collection.key),
        IroError::Holder
    );
    let ix = BurnV1 {
        asset: *asset.key,
        collection: Some(*collection.key),
        payer: *holder.key,
        authority: Some(*holder.key),
        system_program: Some(*system.key),
        log_wrapper: None,
    }
    .instruction(BurnV1InstructionArgs {
        compression_proof: None,
    });
    invoke_signed(&ix, &[asset, collection, holder, system, program], &[]).map_err(Into::into)
}
