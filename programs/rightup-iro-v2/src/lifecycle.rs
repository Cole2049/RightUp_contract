use crate::{contexts::*, core_adapter, math, proof, pump, state::*};
use anchor_lang::{
    prelude::*,
    solana_program::{
        instruction::{AccountMeta, Instruction},
        program::invoke_signed,
    },
};
use anchor_spl::{
    associated_token,
    token_interface::{self, TransferChecked},
};
use solana_sha256_hasher::hashv;

/// Events are self-CPI instruction data, not log lines: the runtime truncates
/// transaction logs but never inner instructions, so the index cannot lose them.
fn publish<'info, E: anchor_lang::Event>(authority: &AccountInfo<'info>, event: E) -> Result<()> {
    let mut data = anchor_lang::event::EVENT_IX_TAG_LE.to_vec();
    data.extend_from_slice(&event.data());
    invoke_signed(
        &Instruction::new_with_bytes(
            crate::ID,
            &data,
            vec![AccountMeta::new_readonly(*authority.key, true)],
        ),
        &[authority.clone()],
        &[&[b"__event_authority", &[crate::EVENT_AUTHORITY_AND_BUMP.1]]],
    )
    .map_err(Into::into)
}

pub fn initialize(
    ctx: Context<InitializePlatform>,
    price_authority: Pubkey,
    draw_authority: Pubkey,
    treasury: Pubkey,
    genesis_hash: [u8; 32],
) -> Result<()> {
    require!(
        price_authority != Pubkey::default()
            && draw_authority != Pubkey::default()
            && treasury != Pubkey::default()
            && ctx.accounts.usdc_mint.key() != ctx.accounts.usdt_mint.key(),
        IroError::Terms
    );
    ctx.accounts.platform.set_inner(Platform {
        admin: ctx.accounts.admin.key(),
        price_authority,
        draw_authority,
        treasury,
        genesis_hash,
        usdc_mint: ctx.accounts.usdc_mint.key(),
        usdt_mint: ctx.accounts.usdt_mint.key(),
        head: 0,
        bump: ctx.bumps.platform,
    });
    Ok(())
}
pub fn update_platform(
    ctx: Context<UpdatePlatform>,
    price_authority: Pubkey,
    draw_authority: Pubkey,
    treasury: Pubkey,
) -> Result<()> {
    require!(
        price_authority != Pubkey::default()
            && draw_authority != Pubkey::default()
            && treasury != Pubkey::default(),
        IroError::Terms
    );
    let p = &mut ctx.accounts.platform;
    p.price_authority = price_authority;
    p.draw_authority = draw_authority;
    p.treasury = treasury;
    publish(
        &ctx.accounts.event_authority,
        PlatformUpdated {
            admin: p.admin,
            price_authority,
            draw_authority,
            treasury,
        },
    )
}
pub fn configure(ctx: Context<CreateConfig>, version: u64, terms: Terms) -> Result<()> {
    require!(
        version > ctx.accounts.platform.head
            && terms.multiplier_bps > 0
            && terms.user_fee_bps <= 10000
            && terms.issuer_fee_bps <= 10000
            && terms.participation_seconds > 0
            && terms.exercise_seconds > 0
            && terms.allocation_count > 0
            && terms.allocation_count <= 10000,
        IroError::Terms
    );
    ctx.accounts.config.set_inner(ConfigVersion {
        version,
        terms: terms.clone(),
    });
    ctx.accounts.platform.head = version;
    publish(
        &ctx.accounts.event_authority,
        ConfigVersionCreated { version, terms },
    )
}
pub fn launch<'info>(ctx: Context<'info, CreatePump<'info>>, args: LaunchArgs) -> Result<()> {
    require!(
        ctx.accounts.project.published_at == 0
            && args.reference.quote_mint == ctx.accounts.project.quote_mint,
        IroError::Completed
    );
    let platform = &ctx.accounts.platform;
    let r = &args.reference;
    if r.quote_mint == pump::WSOL {
        require!(
            r.quote_decimals == 9 && r.quote_micro_u == r.sol_micro_u,
            IroError::Reference
        );
    }
    if r.quote_mint == platform.usdc_mint || r.quote_mint == platform.usdt_mint {
        require!(
            r.quote_decimals == 6 && r.quote_micro_u == 1_000_000,
            IroError::Reference
        );
    }
    let version = ctx.accounts.config.version;
    let terms = ctx.accounts.config.terms.clone();
    let message = proof::reference_message(
        platform,
        &ctx.accounts.project.id,
        &ctx.accounts.creator.key(),
        &ctx.accounts.mint.key(),
        version,
        r,
    )?;
    proof::verify_reference(
        &ctx.accounts.instructions,
        &platform.price_authority,
        &message,
        r,
    )?;
    let split = 16 + args.create_quote_accounts as usize;
    require!(
        ctx.remaining_accounts.len() == split + 27,
        IroError::Account
    );
    let (create_accounts, buy_accounts) = ctx.remaining_accounts.split_at(split);
    pump::create(
        create_accounts,
        &args,
        &ctx.accounts.project,
        ctx.accounts.creator.key(),
        ctx.accounts.mint.key(),
    )?;
    for (ata, authority) in [
        (
            &ctx.accounts.creator_tokens,
            ctx.accounts.creator.to_account_info(),
        ),
        (
            &ctx.accounts.token_vault,
            ctx.accounts.project.to_account_info(),
        ),
    ] {
        associated_token::create_idempotent(CpiContext::new(
            ctx.accounts.associated_token_program.key(),
            associated_token::Create {
                payer: ctx.accounts.creator.to_account_info(),
                associated_token: ata.to_account_info(),
                authority,
                mint: ctx.accounts.mint.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
                token_program: ctx.accounts.token_program.to_account_info(),
            },
        ))?;
    }
    require_keys_eq!(
        *buy_accounts[14].key,
        ctx.accounts.creator_tokens.key(),
        IroError::Account
    );
    let (bought, cost, supply) = pump::buy(
        buy_accounts,
        &args,
        ctx.accounts.creator.key(),
        ctx.accounts.mint.key(),
    )?;
    let escrow =
        u64::try_from((supply as u128) * 5 / 100).map_err(|_| error!(IroError::Arithmetic))?;
    let quota = escrow
        .checked_div(terms.allocation_count as u64)
        .ok_or(IroError::Arithmetic)?;
    require!(quota > 0, IroError::Terms);
    token_interface::transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.creator_tokens.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.token_vault.to_account_info(),
                authority: ctx.accounts.creator.to_account_info(),
            },
        ),
        escrow,
        6,
    )?;
    let actual = pump::token(&ctx.accounts.token_vault, &pump::TOKEN_2022)?;
    require!(
        actual.amount == escrow
            && actual.owner == ctx.accounts.project.key()
            && actual.mint == ctx.accounts.mint.key(),
        IroError::Account
    );
    let base_amounts = math::prices(
        cost,
        bought,
        quota,
        r.quote_micro_u,
        r.quote_decimals,
        r.sol_micro_u,
        terms.multiplier_bps,
    )?;
    let now = Clock::get()?.unix_timestamp;
    let participation_end = now
        .checked_add(terms.participation_seconds as i64)
        .ok_or(IroError::Arithmetic)?;
    let draw_deadline = participation_end
        .checked_add(terms.exercise_seconds as i64)
        .ok_or(IroError::Arithmetic)?;
    let snapshot_hash = proof::snapshot(
        &message,
        version,
        &terms,
        [supply, bought, cost, escrow, quota],
        &base_amounts,
        [now, participation_end, draw_deadline],
    )?;
    let allocation_count = terms.allocation_count;
    let treasury = platform.treasury;
    let p = &mut ctx.accounts.project;
    p.config_version = version;
    p.terms = terms;
    p.treasury = treasury;
    p.total_supply = supply;
    p.first_buy_tokens = bought;
    p.first_buy_cost = cost;
    p.escrow_amount = escrow;
    p.tokens_per_allocation = quota;
    p.base_amounts = base_amounts;
    p.snapshot_hash = snapshot_hash;
    p.published_at = now;
    p.participation_ends_at = participation_end;
    p.draw_deadline = draw_deadline;
    // The exercise window opens when the draw root lands, not at launch.
    p.exercise_ends_at = 0;
    p.claimed = vec![0; claim_bitmap_len(allocation_count)];
    publish(
        &ctx.accounts.event_authority,
        ProjectPublished {
            project: ctx.accounts.project.key(),
            id: ctx.accounts.project.id,
            creator: ctx.accounts.creator.key(),
            mint: ctx.accounts.mint.key(),
            collection: ctx.accounts.project.collection,
            config_version: version,
            snapshot_hash,
            total_supply: supply,
            first_buy_tokens: bought,
            first_buy_cost: cost,
            quote_mint: r.quote_mint,
            escrow_amount: escrow,
            tokens_per_allocation: quota,
            allocation_count,
            base_amounts,
            published_at: now,
            participation_ends_at: participation_end,
            draw_deadline,
            quote_micro_u: r.quote_micro_u,
            sol_micro_u: r.sol_micro_u,
            observed_at: r.observed_at,
            expires_at: r.expires_at,
        },
    )
}
pub fn draw(ctx: Context<CommitDraw>, draw_id: [u8; 16], root: [u8; 32], count: u32) -> Result<()> {
    let p = &mut ctx.accounts.project;
    require!(p.published_at > 0, IroError::Period);
    if p.draw_committed {
        // Resubmitting the same result is a no-op and never moves the deadline.
        require!(
            p.draw_id == draw_id && p.draw_root == root && p.allocated_count == count,
            IroError::Completed
        );
        return Ok(());
    }
    let now = Clock::get()?.unix_timestamp;
    require!(
        now >= p.participation_ends_at && now < p.draw_deadline,
        IroError::Period
    );
    require!(
        count <= p.terms.allocation_count && (count == 0 || root != [0; 32]),
        IroError::Proof
    );
    // The full exercise window starts here; with nothing allocated it is already over.
    let exercise_ends_at = if count == 0 {
        now
    } else {
        now.checked_add(p.terms.exercise_seconds as i64)
            .ok_or(IroError::Arithmetic)?
    };
    p.draw_id = draw_id;
    p.draw_root = root;
    p.allocated_count = count;
    p.draw_committed = true;
    p.exercise_ends_at = exercise_ends_at;
    publish(
        &ctx.accounts.event_authority,
        DrawCommitted {
            project: p.key(),
            draw_id,
            root,
            allocated_count: count,
            exercise_ends_at,
        },
    )
}
/// Byte and mask of an index in the claim bitmap.
fn claim_bit(claimed: &[u8], index: u32) -> Result<(usize, u8)> {
    let byte = (index / 8) as usize;
    require!(byte < claimed.len(), IroError::Proof);
    Ok((byte, 1 << (index % 8)))
}
fn is_claimed(claimed: &[u8], index: u32) -> Result<bool> {
    let (byte, mask) = claim_bit(claimed, index)?;
    Ok(claimed[byte] & mask != 0)
}
pub fn claim(ctx: Context<ClaimAllocation>, index: u32, nodes: Vec<[u8; 32]>) -> Result<()> {
    let p = &ctx.accounts.project;
    let now = Clock::get()?.unix_timestamp;
    require!(
        p.draw_committed && index < p.allocated_count && now < p.exercise_ends_at,
        IroError::Period
    );
    // The bitmap, not the asset account, is what makes a claim one-time: it does
    // not depend on how Core leaves an account after a burn.
    require!(!is_claimed(&p.claimed, index)?, IroError::Completed);
    proof::verify_merkle(
        proof::allocation_leaf(
            &p.key(),
            index,
            &ctx.accounts.claimant.key(),
            p.tokens_per_allocation,
            &p.snapshot_hash,
        ),
        &nodes,
        p.draw_root,
    )?;
    let project_key = p.key();
    let index_bytes = index.to_le_bytes();
    let asset_bump = [ctx.bumps.asset];
    let asset_seeds: &[&[u8]] = &[ASSET_SEED, project_key.as_ref(), &index_bytes, &asset_bump];
    let asset = ctx.accounts.asset.to_account_info();
    // Anyone can send lamports to the asset address beforehand. Hand them to the
    // claimant so that a deposit can never stop Core from creating the asset.
    let deposit = asset.lamports();
    if deposit > 0 {
        require!(
            asset.owner == &anchor_lang::system_program::ID && asset.data_is_empty(),
            IroError::Completed
        );
        anchor_lang::system_program::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.system_program.key(),
                anchor_lang::system_program::Transfer {
                    from: asset.clone(),
                    to: ctx.accounts.claimant.to_account_info(),
                },
                &[asset_seeds],
            ),
            deposit,
        )?;
    }
    let project_seeds: &[&[u8]] = &[b"v2-project", &p.id, &[p.bump]];
    core_adapter::claim(
        asset,
        ctx.accounts.collection.to_account_info(),
        p.to_account_info(),
        ctx.accounts.claimant.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.core_program.to_account_info(),
        p.nft_name.clone(),
        p.nft_uri.clone(),
        &[project_seeds, asset_seeds],
    )?;
    let (byte, mask) = claim_bit(&ctx.accounts.project.claimed, index)?;
    ctx.accounts.project.claimed[byte] |= mask;
    publish(
        &ctx.accounts.event_authority,
        AllocationClaimed {
            project: project_key,
            index,
            asset: ctx.accounts.asset.key(),
            claim_wallet: ctx.accounts.claimant.key(),
        },
    )
}
/// An allocation is exercisable while the window is open, once it was claimed
/// and while its asset exists. A burned asset leaves no exercisable account.
fn validate_exercise(p: &Project, index: u32, asset: &AccountInfo) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(
        p.draw_committed && now < p.exercise_ends_at,
        IroError::Period
    );
    require!(is_claimed(&p.claimed, index)?, IroError::Holder);
    require!(asset.data_len() > 1, IroError::Completed);
    Ok(())
}
/// Income and unsold tokens settle once the exercise window is over, or once the
/// draw deadline passed without any committed root.
fn validate_settlement(p: &Project) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(
        p.published_at > 0
            && ((p.draw_committed && now >= p.exercise_ends_at)
                || (!p.draw_committed && now >= p.draw_deadline)),
        IroError::Period
    );
    Ok(())
}
fn release<'info>(
    p: &Account<'info, Project>,
    program: AccountInfo<'info>,
    from: AccountInfo<'info>,
    mint: AccountInfo<'info>,
    to: AccountInfo<'info>,
    amount: u64,
) -> Result<()> {
    let signer: &[&[u8]] = &[b"v2-project", &p.id, &[p.bump]];
    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            *program.key,
            TransferChecked {
                from,
                mint,
                to,
                authority: p.to_account_info(),
            },
            &[signer],
        ),
        amount,
        p.token_decimals,
    )
}
#[allow(clippy::too_many_arguments)]
fn record<'info>(
    p: &Account<'info, Project>,
    l: &mut Account<'info, PaymentLedger>,
    event_authority: &AccountInfo<'info>,
    index: u32,
    asset: Pubkey,
    holder: Pubkey,
    base: u64,
    fee: u64,
) -> Result<()> {
    l.gross_base = l.gross_base.checked_add(base).ok_or(IroError::Arithmetic)?;
    l.user_fees = l.user_fees.checked_add(fee).ok_or(IroError::Arithmetic)?;
    l.exercised_count = l
        .exercised_count
        .checked_add(1)
        .ok_or(IroError::Arithmetic)?;
    publish(
        event_authority,
        AllocationExercised {
            project: p.key(),
            index,
            asset,
            holder,
            currency: l.currency,
            base_amount: base,
            user_fee: fee,
            token_amount: p.tokens_per_allocation,
        },
    )
}
pub fn exercise_sol(ctx: Context<ExerciseSol>, index: u32) -> Result<()> {
    let p = &ctx.accounts.project;
    validate_exercise(p, index, &ctx.accounts.asset)?;
    let base = p.base_amounts[0];
    let fee = math::fee(base, p.terms.user_fee_bps, true)?;
    for (to, amount) in [
        (ctx.accounts.ledger.to_account_info(), base),
        (ctx.accounts.treasury.to_account_info(), fee),
    ] {
        if amount > 0 {
            anchor_lang::system_program::transfer(
                CpiContext::new(
                    ctx.accounts.system_program.key(),
                    anchor_lang::system_program::Transfer {
                        from: ctx.accounts.holder.to_account_info(),
                        to,
                    },
                ),
                amount,
            )?;
        }
    }
    core_adapter::burn(
        ctx.accounts.asset.to_account_info(),
        ctx.accounts.collection.to_account_info(),
        ctx.accounts.holder.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.core_program.to_account_info(),
    )?;
    release(
        p,
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.token_vault.to_account_info(),
        ctx.accounts.mint.to_account_info(),
        ctx.accounts.holder_tokens.to_account_info(),
        p.tokens_per_allocation,
    )?;
    record(
        p,
        &mut ctx.accounts.ledger,
        &ctx.accounts.event_authority,
        index,
        ctx.accounts.asset.key(),
        ctx.accounts.holder.key(),
        base,
        fee,
    )
}
pub fn exercise_token(ctx: Context<ExerciseToken>, currency: u8, index: u32) -> Result<()> {
    let p = &ctx.accounts.project;
    validate_exercise(p, index, &ctx.accounts.asset)?;
    require!(currency == 1 || currency == 2, IroError::Currency);
    let base = p.base_amounts[currency as usize];
    let fee = math::fee(base, p.terms.user_fee_bps, true)?;
    for (to, amount) in [
        (ctx.accounts.payment_vault.to_account_info(), base),
        (ctx.accounts.treasury_tokens.to_account_info(), fee),
    ] {
        if amount > 0 {
            token_interface::transfer_checked(
                CpiContext::new(
                    ctx.accounts.payment_program.key(),
                    TransferChecked {
                        from: ctx.accounts.holder_payment.to_account_info(),
                        mint: ctx.accounts.payment_mint.to_account_info(),
                        to,
                        authority: ctx.accounts.holder.to_account_info(),
                    },
                ),
                amount,
                6,
            )?;
        }
    }
    core_adapter::burn(
        ctx.accounts.asset.to_account_info(),
        ctx.accounts.collection.to_account_info(),
        ctx.accounts.holder.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.core_program.to_account_info(),
    )?;
    release(
        p,
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.token_vault.to_account_info(),
        ctx.accounts.mint.to_account_info(),
        ctx.accounts.holder_tokens.to_account_info(),
        p.tokens_per_allocation,
    )?;
    record(
        p,
        &mut ctx.accounts.ledger,
        &ctx.accounts.event_authority,
        index,
        ctx.accounts.asset.key(),
        ctx.accounts.holder.key(),
        base,
        fee,
    )
}
fn validate_withdraw(p: &Project, l: &PaymentLedger) -> Result<()> {
    validate_settlement(p)?;
    require!(!l.withdrawn, IroError::Completed);
    Ok(())
}
pub fn withdraw_sol(ctx: Context<WithdrawSol>) -> Result<()> {
    let p = &ctx.accounts.project;
    let l = &mut ctx.accounts.ledger;
    validate_withdraw(p, l)?;
    let fee = math::fee(l.gross_base, p.terms.issuer_fee_bps, false)?;
    let net = l.gross_base.checked_sub(fee).ok_or(IroError::Arithmetic)?;
    require!(
        l.to_account_info().lamports()
            >= Rent::get()?
                .minimum_balance(l.to_account_info().data_len())
                .checked_add(l.gross_base)
                .ok_or(IroError::Arithmetic)?,
        IroError::Account
    );
    for (to, amount) in [
        (ctx.accounts.creator.to_account_info(), net),
        (ctx.accounts.treasury.to_account_info(), fee),
    ] {
        l.sub_lamports(amount)?;
        to.add_lamports(amount)?;
    }
    l.withdrawn = true;
    publish(
        &ctx.accounts.event_authority,
        IncomeWithdrawn {
            project: p.key(),
            currency: 0,
            gross_base: l.gross_base,
            issuer_fee: fee,
            net,
        },
    )
}
pub fn withdraw_token(ctx: Context<WithdrawToken>, currency: u8) -> Result<()> {
    let p = &ctx.accounts.project;
    let l = &mut ctx.accounts.ledger;
    require!(currency == 1 || currency == 2, IroError::Currency);
    validate_withdraw(p, l)?;
    let fee = math::fee(l.gross_base, p.terms.issuer_fee_bps, false)?;
    let net = l.gross_base.checked_sub(fee).ok_or(IroError::Arithmetic)?;
    let signer: &[&[u8]] = &[
        b"v2-payment",
        p.to_account_info().key.as_ref(),
        &[currency],
        &[l.bump],
    ];
    for (to, amount) in [
        (ctx.accounts.creator_tokens.to_account_info(), net),
        (ctx.accounts.treasury_tokens.to_account_info(), fee),
    ] {
        if amount > 0 {
            token_interface::transfer_checked(
                CpiContext::new_with_signer(
                    ctx.accounts.payment_program.key(),
                    TransferChecked {
                        from: ctx.accounts.payment_vault.to_account_info(),
                        mint: ctx.accounts.payment_mint.to_account_info(),
                        to,
                        authority: l.to_account_info(),
                    },
                    &[signer],
                ),
                amount,
                6,
            )?;
        }
    }
    l.withdrawn = true;
    publish(
        &ctx.accounts.event_authority,
        IncomeWithdrawn {
            project: p.key(),
            currency,
            gross_base: l.gross_base,
            issuer_fee: fee,
            net,
        },
    )
}
pub fn return_tokens(ctx: Context<ReturnTokens>) -> Result<()> {
    let p = &ctx.accounts.project;
    validate_settlement(p)?;
    require!(!p.remaining_returned, IroError::Completed);
    let exercised = (ctx.accounts.sol_ledger.exercised_count as u64)
        .checked_add(ctx.accounts.usdc_ledger.exercised_count as u64)
        .and_then(|n| n.checked_add(ctx.accounts.usdt_ledger.exercised_count as u64))
        .ok_or(IroError::Arithmetic)?;
    let amount = p
        .escrow_amount
        .checked_sub(
            exercised
                .checked_mul(p.tokens_per_allocation)
                .ok_or(IroError::Arithmetic)?,
        )
        .ok_or(IroError::Arithmetic)?;
    require!(ctx.accounts.token_vault.amount >= amount, IroError::Account);
    release(
        p,
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.token_vault.to_account_info(),
        ctx.accounts.mint.to_account_info(),
        ctx.accounts.creator_tokens.to_account_info(),
        amount,
    )?;
    ctx.accounts.project.remaining_returned = true;
    publish(
        &ctx.accounts.event_authority,
        RemainingTokensReturned {
            project: ctx.accounts.project.key(),
            amount,
        },
    )
}

pub fn prepare(ctx: Context<PreparePump>, args: PrepareArgs) -> Result<()> {
    require!(
        !args.name.is_empty()
            && args.name.len() <= 32
            && !args.symbol.is_empty()
            && args.symbol.len() <= 10
            && args.token_uri.len() <= MAX_URI_LEN
            && args.collection_uri.len() <= MAX_URI_LEN
            && args.nft_uri.len() <= MAX_URI_LEN
            && args.nft_uri.starts_with("ipfs://")
            && args.token_uri.starts_with("ipfs://")
            && args.collection_uri.starts_with("ipfs://"),
        IroError::Terms
    );
    let hashes = [
        hashv(&[args.name.as_bytes()]).to_bytes(),
        hashv(&[args.symbol.as_bytes()]).to_bytes(),
        hashv(&[args.token_uri.as_bytes()]).to_bytes(),
        hashv(&[args.collection_uri.as_bytes()]).to_bytes(),
        hashv(&[args.nft_uri.as_bytes()]).to_bytes(),
    ];
    let platform = &ctx.accounts.platform;
    let bump = ctx.bumps.project;
    let signer: &[&[u8]] = &[b"v2-project", &args.id, &[bump]];
    core_adapter::collection(
        ctx.accounts.collection.to_account_info(),
        ctx.accounts.project.to_account_info(),
        ctx.accounts.creator.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.core_program.to_account_info(),
        args.name.clone(),
        args.collection_uri,
        &[signer],
    )?;
    ctx.accounts.project.set_inner(Project {
        id: args.id,
        creator: ctx.accounts.creator.key(),
        mint: ctx.accounts.mint.key(),
        token_program: pump::TOKEN_2022,
        collection: ctx.accounts.collection.key(),
        // Version, terms and treasury are fixed by create_pump_iro, not here.
        config_version: 0,
        treasury: Pubkey::default(),
        usdc_mint: platform.usdc_mint,
        usdt_mint: platform.usdt_mint,
        terms: Terms {
            multiplier_bps: 0,
            user_fee_bps: 0,
            issuer_fee_bps: 0,
            participation_seconds: 0,
            exercise_seconds: 0,
            allocation_count: 0,
            digest: [0; 32],
        },
        token_decimals: 6,
        total_supply: 0,
        first_buy_tokens: 0,
        first_buy_cost: 0,
        quote_mint: args.quote_mint,
        escrow_amount: 0,
        tokens_per_allocation: 0,
        base_amounts: [0; 3],
        snapshot_hash: [0; 32],
        published_at: 0,
        participation_ends_at: 0,
        draw_deadline: 0,
        exercise_ends_at: 0,
        draw_id: [0; 16],
        draw_root: [0; 32],
        allocated_count: 0,
        draw_committed: false,
        remaining_returned: false,
        bump,
        nft_name: args.name,
        symbol: args.symbol,
        token_uri: args.token_uri,
        holder_rewards: args.holder_rewards,
        creator_fee_bps: args.creator_fee_bps,
        nft_uri: args.nft_uri,
        // Sized when the allocation count is locked at launch.
        claimed: Vec::new(),
    });
    for (l, currency, bump) in [
        (&mut ctx.accounts.sol_ledger, 0, ctx.bumps.sol_ledger),
        (&mut ctx.accounts.usdc_ledger, 1, ctx.bumps.usdc_ledger),
        (&mut ctx.accounts.usdt_ledger, 2, ctx.bumps.usdt_ledger),
    ] {
        l.set_inner(PaymentLedger {
            project: ctx.accounts.project.key(),
            currency,
            gross_base: 0,
            user_fees: 0,
            exercised_count: 0,
            withdrawn: false,
            bump,
        });
    }
    publish(
        &ctx.accounts.event_authority,
        ProjectPrepared {
            project: ctx.accounts.project.key(),
            id: args.id,
            creator: ctx.accounts.creator.key(),
            mint: ctx.accounts.mint.key(),
            collection: ctx.accounts.collection.key(),
            quote_mint: args.quote_mint,
            holder_rewards: args.holder_rewards,
            creator_fee_bps: args.creator_fee_bps,
            name_hash: hashes[0],
            symbol_hash: hashes[1],
            token_uri_hash: hashes[2],
            collection_uri_hash: hashes[3],
            nft_uri_hash: hashes[4],
        },
    )
}
