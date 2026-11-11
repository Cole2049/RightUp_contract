//! Fixed Pump ABI, pinned to official docs cb188ce08b5069196eef1f3e4a0c43b70099793b.
//! Callers supply accounts, never arbitrary instruction bytes or program IDs.
use crate::state::*;
use anchor_lang::{
    prelude::*,
    solana_program::{
        instruction::{AccountMeta, Instruction},
        program::invoke,
    },
};
use anchor_spl::token_interface::{Mint, TokenAccount};

pub const PUMP_ID: Pubkey = pubkey!("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
pub const WSOL: Pubkey = pubkey!("So11111111111111111111111111111111111111112");
pub const TOKEN_2022: Pubkey = pubkey!("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

#[derive(AnchorDeserialize)]
struct Curve {
    _virtual_tokens: u64,
    _virtual_quote: u64,
    _real_tokens: u64,
    real_quote: u64,
    supply: u64,
    _complete: bool,
    _creator: Pubkey,
}
fn curve(account: &AccountInfo) -> Result<Curve> {
    require_keys_eq!(*account.owner, PUMP_ID, IroError::Account);
    let data = account.try_borrow_data()?;
    require!(
        data.len() >= 8 && data[..8] == [23, 183, 248, 55, 96, 216, 172, 96],
        IroError::Account
    );
    Curve::deserialize(&mut &data[8..]).map_err(|_| error!(IroError::Account))
}
pub fn token(account: &AccountInfo, program: &Pubkey) -> Result<TokenAccount> {
    require_keys_eq!(*account.owner, *program, IroError::Account);
    TokenAccount::try_deserialize(&mut &account.try_borrow_data()?[..])
}
pub fn mint(account: &AccountInfo, program: &Pubkey) -> Result<Mint> {
    require_keys_eq!(*account.owner, *program, IroError::Account);
    Mint::try_deserialize(&mut &account.try_borrow_data()?[..])
}
fn cpi<'info>(
    accounts: &[AccountInfo<'info>],
    data: Vec<u8>,
    writable: &[usize],
    signers: &[usize],
) -> Result<()> {
    let metas = accounts
        .iter()
        .enumerate()
        .map(|(i, a)| {
            if writable.contains(&i) {
                AccountMeta::new(*a.key, signers.contains(&i))
            } else {
                AccountMeta::new_readonly(*a.key, signers.contains(&i))
            }
        })
        .collect();
    invoke(
        &Instruction {
            program_id: PUMP_ID,
            accounts: metas,
            data,
        },
        accounts,
    )
    .map_err(Into::into)
}
pub fn create<'info>(
    accounts: &[AccountInfo<'info>],
    args: &LaunchArgs,
    project: &Project,
    creator: Pubkey,
    mint: Pubkey,
) -> Result<()> {
    require!(
        args.create_quote_accounts <= 4
            && (args.create_quote_accounts == 0 || args.create_quote_accounts >= 3)
            && accounts.len() == 16 + args.create_quote_accounts as usize,
        IroError::Account
    );
    require_keys_eq!(*accounts[0].key, mint, IroError::Account);
    require_keys_eq!(*accounts[5].key, creator, IroError::Account);
    require_keys_eq!(*accounts[7].key, TOKEN_2022, IroError::Account);
    require_keys_eq!(*accounts[15].key, PUMP_ID, IroError::Account);
    require!(
        (args.reference.quote_mint == WSOL) == (args.create_quote_accounts == 0),
        IroError::Account
    );
    if args.create_quote_accounts > 0 {
        require_keys_eq!(
            *accounts[16].key,
            args.reference.quote_mint,
            IroError::Account
        );
    }
    let mut data = vec![214, 144, 76, 236, 95, 139, 49, 180];
    project.nft_name.serialize(&mut data)?;
    project.symbol.serialize(&mut data)?;
    project.token_uri.serialize(&mut data)?;
    creator.serialize(&mut data)?;
    false.serialize(&mut data)?; // ordinary Pump creation, no mayhem supply changes
    false.serialize(&mut data)?; // Pump OptionBool is a one-field struct, not Rust Option
    project.creator_fee_bps.serialize(&mut data)?;
    project.holder_rewards.serialize(&mut data)?;
    cpi(accounts, data, &[0, 2, 3, 5, 9, 11, 12, 13, 17], &[0, 5])
}
fn fee_balance(
    account: &AccountInfo,
    native: bool,
    quote: &Pubkey,
    quote_program: &Pubkey,
) -> Result<u64> {
    if native {
        if account.lamports() == 0 {
            return Ok(0);
        }
        if account.owner == &anchor_lang::system_program::ID && account.data_is_empty() {
            return Ok(account.lamports());
        }
        // Creator vault initialization rent is not a purchase fee.
        Ok(account
            .lamports()
            .checked_sub(Rent::get()?.minimum_balance(account.data_len()))
            .unwrap_or(0))
    } else if account.data_is_empty() {
        Ok(0)
    } else {
        let t = token(account, quote_program)?;
        require_keys_eq!(t.mint, *quote, IroError::Account);
        Ok(t.amount)
    }
}
pub fn buy<'info>(
    accounts: &[AccountInfo<'info>],
    args: &LaunchArgs,
    creator: Pubkey,
    mint_key: Pubkey,
) -> Result<(u64, u64, u64)> {
    require!(accounts.len() == 27, IroError::Account);
    for (i, key) in [
        (1, mint_key),
        (2, args.reference.quote_mint),
        (3, TOKEN_2022),
        (13, creator),
        (26, PUMP_ID),
    ] {
        require_keys_eq!(*accounts[i].key, key, IroError::Account);
    }
    let curve_key =
        Pubkey::find_program_address(&[b"bonding-curve", mint_key.as_ref()], &PUMP_ID).0;
    require_keys_eq!(*accounts[10].key, curve_key, IroError::Account);
    let native = args.reference.quote_mint == WSOL;
    let quote_program = *accounts[4].key;
    require!(
        quote_program == anchor_spl::token::ID || quote_program == TOKEN_2022,
        IroError::Account
    );
    let quote = mint(&accounts[2], &quote_program)?;
    require!(
        quote.decimals == args.reference.quote_decimals,
        IroError::Reference
    );
    let prior_quote = if native {
        0
    } else {
        let t = token(&accounts[15], &quote_program)?;
        require!(
            t.owner == creator && t.mint == args.reference.quote_mint,
            IroError::Account
        );
        t.amount
    };
    let prior_curve = curve(&accounts[10])?;
    require!(prior_curve.real_quote == 0, IroError::FirstBuy);
    let prior = token(&accounts[14], &TOKEN_2022)?;
    require!(
        prior.owner == creator && prior.mint == mint_key && prior.amount == 0,
        IroError::FirstBuy
    );
    let fee_indices = if native { [6, 8, 16] } else { [7, 9, 17] };
    let mut previous = [0u64; 3];
    for (j, i) in fee_indices.iter().enumerate() {
        previous[j] = fee_balance(
            &accounts[*i],
            native,
            &args.reference.quote_mint,
            &quote_program,
        )?;
    }
    let mut data = vec![184, 23, 238, 97, 103, 197, 211, 61];
    args.first_buy_amount.serialize(&mut data)?;
    args.max_quote_cost.serialize(&mut data)?;
    cpi(
        accounts,
        data,
        &[6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 20, 21],
        &[13],
    )?;
    let after_curve = curve(&accounts[10])?;
    let bought = token(&accounts[14], &TOKEN_2022)?.amount;
    let mut cost = after_curve
        .real_quote
        .checked_sub(prior_curve.real_quote)
        .ok_or(IroError::Arithmetic)?;
    for (j, i) in fee_indices.iter().enumerate() {
        if fee_indices[..j]
            .iter()
            .any(|p| accounts[*p].key == accounts[*i].key)
        {
            continue;
        }
        let mut received = fee_balance(
            &accounts[*i],
            native,
            &args.reference.quote_mint,
            &quote_program,
        )?
        .checked_sub(previous[j])
        .ok_or(IroError::Arithmetic)?;
        // A fee account this buy opened (the creator's vault on a creator's first
        // launch) also holds the rent that opened it; rent is not a purchase fee.
        if native
            && previous[j] == 0
            && accounts[*i].owner == &anchor_lang::system_program::ID
            && accounts[*i].data_is_empty()
        {
            received = received.saturating_sub(Rent::get()?.minimum_balance(0));
        }
        cost = cost.checked_add(received).ok_or(IroError::Arithmetic)?;
    }
    if !native {
        cost = prior_quote
            .checked_sub(token(&accounts[15], &quote_program)?.amount)
            .ok_or(IroError::Arithmetic)?;
    }
    require!(
        cost > 0
            && cost <= args.max_quote_cost
            && bought > 0
            && (bought as u128) * 100 >= (after_curve.supply as u128) * 10,
        IroError::FirstBuy
    );
    let state = mint(&accounts[1], &TOKEN_2022)?;
    require!(
        state.supply == after_curve.supply && state.decimals == 6,
        IroError::Account
    );
    Ok((bought, cost, state.supply))
}
