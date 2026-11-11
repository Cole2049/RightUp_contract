use crate::{core_adapter, pump, state::*};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::{self, AssociatedToken},
    token_interface::{Mint, TokenAccount, TokenInterface},
};

#[derive(Accounts)]
pub struct InitializePlatform<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(constraint=program.programdata_address()? == Some(program_data.key()) @ IroError::Authority)]
    pub program: Program<'info, crate::program::RightupIroV2>,
    #[account(constraint=program_data.upgrade_authority_address == Some(admin.key()) @ IroError::Authority)]
    pub program_data: Account<'info, ProgramData>,
    #[account(init,payer=admin,space=8+Platform::INIT_SPACE,seeds=[b"v2-platform".as_ref()],bump)]
    pub platform: Box<Account<'info, Platform>>,
    #[account(mint::decimals=6,mint::token_program=payment_program)]
    pub usdc_mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mint::decimals=6,mint::token_program=payment_program)]
    pub usdt_mint: Box<InterfaceAccount<'info, Mint>>,
    pub payment_program: Program<'info, anchor_spl::token::Token>,
    pub system_program: Program<'info, System>,
}
#[event_cpi]
#[derive(Accounts)]
pub struct UpdatePlatform<'info> {
    pub admin: Signer<'info>,
    #[account(mut,seeds=[b"v2-platform".as_ref()],bump=platform.bump,has_one=admin @ IroError::Authority)]
    pub platform: Box<Account<'info, Platform>>,
}
#[event_cpi]
#[derive(Accounts)]
#[instruction(version:u64)]
pub struct CreateConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut,seeds=[b"v2-platform".as_ref()],bump=platform.bump,has_one=admin)]
    pub platform: Box<Account<'info, Platform>>,
    #[account(init,payer=admin,space=8+ConfigVersion::INIT_SPACE,seeds=[b"v2-config".as_ref(),&version.to_le_bytes()],bump)]
    pub config: Box<Account<'info, ConfigVersion>>,
    pub system_program: Program<'info, System>,
}
#[event_cpi]
#[derive(Accounts)]
#[instruction(args:PrepareArgs)]
pub struct PreparePump<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    /// Platform publish authority: no project exists without a backend-built preparation.
    #[account(constraint=authority.key()==platform.price_authority @ IroError::Authority)]
    pub authority: Signer<'info>,
    #[account(seeds=[b"v2-platform".as_ref()],bump=platform.bump)]
    pub platform: Box<Account<'info, Platform>>,
    #[account(init,payer=creator,space=8+Project::INIT_SPACE,seeds=[b"v2-project".as_ref(),&args.id],bump)]
    pub project: Box<Account<'info, Project>>,
    /// CHECK: future Pump mint public key; actual signer/create is required at launch.
    pub mint: UncheckedAccount<'info>,
    #[account(mut)]
    pub collection: Signer<'info>,
    #[account(init,payer=creator,space=8+PaymentLedger::INIT_SPACE,seeds=[b"v2-payment".as_ref(),project.key().as_ref(),&[0]],bump)]
    pub sol_ledger: Box<Account<'info, PaymentLedger>>,
    #[account(init,payer=creator,space=8+PaymentLedger::INIT_SPACE,seeds=[b"v2-payment".as_ref(),project.key().as_ref(),&[1]],bump)]
    pub usdc_ledger: Box<Account<'info, PaymentLedger>>,
    #[account(init,payer=creator,space=8+PaymentLedger::INIT_SPACE,seeds=[b"v2-payment".as_ref(),project.key().as_ref(),&[2]],bump)]
    pub usdt_ledger: Box<Account<'info, PaymentLedger>>,
    /// CHECK: fixed executable Core program.
    #[account(address=core_adapter::CORE_ID,executable)]
    pub core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}
#[event_cpi]
#[derive(Accounts)]
pub struct CreatePump<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(seeds=[b"v2-platform".as_ref()],bump=platform.bump)]
    pub platform: Box<Account<'info, Platform>>,
    /// Terms lock at launch: only the current head version is accepted.
    #[account(seeds=[b"v2-config".as_ref(),&config.version.to_le_bytes()],bump,constraint=config.version==platform.head @ IroError::Terms)]
    pub config: Box<Account<'info, ConfigVersion>>,
    /// Grows by the claim bitmap for the locked allocation count; the dev pays its rent.
    #[account(mut,seeds=[b"v2-project".as_ref(),&project.id],bump=project.bump,has_one=creator,has_one=mint,
        realloc=8+Project::INIT_SPACE+claim_bitmap_len(config.terms.allocation_count),realloc::payer=creator,realloc::zero=false)]
    pub project: Box<Account<'info, Project>>,
    #[account(mut)]
    pub mint: Signer<'info>,
    /// CHECK: canonical creator ATA, initialized after mint creation and parsed.
    #[account(mut,address=associated_token::get_associated_token_address_with_program_id(&creator.key(),&mint.key(),&pump::TOKEN_2022))]
    pub creator_tokens: UncheckedAccount<'info>,
    /// CHECK: canonical vault ATA, initialized after mint creation and parsed.
    #[account(mut,address=associated_token::get_associated_token_address_with_program_id(&project.key(),&mint.key(),&pump::TOKEN_2022))]
    pub token_vault: UncheckedAccount<'info>,
    /// CHECK: fixed instructions sysvar for Ed25519 reference verification.
    #[account(address=solana_sdk_ids::sysvar::instructions::ID)]
    pub instructions: UncheckedAccount<'info>,
    pub token_program: Program<'info, anchor_spl::token_2022::Token2022>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
#[event_cpi]
#[derive(Accounts)]
pub struct CommitDraw<'info> {
    pub authority: Signer<'info>,
    /// Current platform authority, so rotation applies to projects already in flight.
    #[account(seeds=[b"v2-platform".as_ref()],bump=platform.bump,constraint=platform.draw_authority==authority.key() @ IroError::Authority)]
    pub platform: Box<Account<'info, Platform>>,
    #[account(mut,seeds=[b"v2-project".as_ref(),&project.id],bump=project.bump)]
    pub project: Box<Account<'info, Project>>,
}
#[event_cpi]
#[derive(Accounts)]
#[instruction(index:u32)]
pub struct ClaimAllocation<'info> {
    #[account(mut)]
    pub claimant: Signer<'info>,
    /// Writable for the claim bitmap.
    #[account(mut,seeds=[b"v2-project".as_ref(),&project.id],bump=project.bump)]
    pub project: Box<Account<'info, Project>>,
    /// CHECK: the allocation's Core asset at its program address, created here by Core.
    #[account(mut,seeds=[ASSET_SEED,project.key().as_ref(),&index.to_le_bytes()],bump)]
    pub asset: UncheckedAccount<'info>,
    /// CHECK: fixed project collection; Core CPI validates its structure.
    #[account(mut,address=project.collection,owner=core_adapter::CORE_ID)]
    pub collection: UncheckedAccount<'info>,
    /// CHECK: fixed executable Core program.
    #[account(address=core_adapter::CORE_ID,executable)]
    pub core_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}
#[event_cpi]
#[derive(Accounts)]
#[instruction(index:u32)]
pub struct ExerciseSol<'info> {
    #[account(mut)]
    pub holder: Signer<'info>,
    #[account(seeds=[b"v2-project".as_ref(),&project.id],bump=project.bump)]
    pub project: Box<Account<'info, Project>>,
    /// CHECK: the allocation's Core asset at its program address; current owner and Collection parsed before burn.
    #[account(mut,owner=core_adapter::CORE_ID,seeds=[ASSET_SEED,project.key().as_ref(),&index.to_le_bytes()],bump)]
    pub asset: UncheckedAccount<'info>,
    /// CHECK: fixed project collection.
    #[account(mut,address=project.collection,owner=core_adapter::CORE_ID)]
    pub collection: UncheckedAccount<'info>,
    /// CHECK: locked fee recipient.
    #[account(mut,address=project.treasury)]
    pub treasury: UncheckedAccount<'info>,
    #[account(address=project.mint,mint::token_program=token_program)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut,associated_token::mint=mint,associated_token::authority=project,associated_token::token_program=token_program)]
    pub token_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(init_if_needed,payer=holder,associated_token::mint=mint,associated_token::authority=holder,associated_token::token_program=token_program)]
    pub holder_tokens: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(address=project.token_program)]
    pub token_program: Interface<'info, TokenInterface>,
    /// CHECK: fixed executable Core program.
    #[account(address=core_adapter::CORE_ID,executable)]
    pub core_program: UncheckedAccount<'info>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    #[account(mut,seeds=[b"v2-payment".as_ref(),project.key().as_ref(),&[0]],bump=ledger.bump,has_one=project,constraint=ledger.currency==0)]
    pub ledger: Box<Account<'info, PaymentLedger>>,
}
#[event_cpi]
#[derive(Accounts)]
#[instruction(currency:u8,index:u32)]
pub struct ExerciseToken<'info> {
    #[account(mut)]
    pub holder: Signer<'info>,
    #[account(seeds=[b"v2-project".as_ref(),&project.id],bump=project.bump)]
    pub project: Box<Account<'info, Project>>,
    /// CHECK: the allocation's Core asset at its program address; current owner and Collection parsed before burn.
    #[account(mut,owner=core_adapter::CORE_ID,seeds=[ASSET_SEED,project.key().as_ref(),&index.to_le_bytes()],bump)]
    pub asset: UncheckedAccount<'info>,
    /// CHECK: fixed project collection.
    #[account(mut,address=project.collection,owner=core_adapter::CORE_ID)]
    pub collection: UncheckedAccount<'info>,
    /// CHECK: locked fee recipient.
    #[account(mut,address=project.treasury)]
    pub treasury: UncheckedAccount<'info>,
    #[account(address=project.mint,mint::token_program=token_program)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut,associated_token::mint=mint,associated_token::authority=project,associated_token::token_program=token_program)]
    pub token_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(init_if_needed,payer=holder,associated_token::mint=mint,associated_token::authority=holder,associated_token::token_program=token_program)]
    pub holder_tokens: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(address=project.token_program)]
    pub token_program: Interface<'info, TokenInterface>,
    /// CHECK: fixed executable Core program.
    #[account(address=core_adapter::CORE_ID,executable)]
    pub core_program: UncheckedAccount<'info>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    #[account(mut,seeds=[b"v2-payment".as_ref(),project.key().as_ref(),&[currency]],bump=ledger.bump,has_one=project,constraint=ledger.currency==currency)]
    pub ledger: Box<Account<'info, PaymentLedger>>,

    #[account(constraint=(currency==1 && payment_mint.key()==project.usdc_mint)||(currency==2 && payment_mint.key()==project.usdt_mint),mint::decimals=6,mint::token_program=payment_program)]
    pub payment_mint: Box<InterfaceAccount<'info, Mint>>,
    pub payment_program: Program<'info, anchor_spl::token::Token>,
    #[account(mut,associated_token::mint=payment_mint,associated_token::authority=holder,associated_token::token_program=payment_program)]
    pub holder_payment: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(init_if_needed,payer=holder,associated_token::mint=payment_mint,associated_token::authority=ledger,associated_token::token_program=payment_program)]
    pub payment_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(init_if_needed,payer=holder,associated_token::mint=payment_mint,associated_token::authority=treasury,associated_token::token_program=payment_program)]
    pub treasury_tokens: Box<InterfaceAccount<'info, TokenAccount>>,
}
#[event_cpi]
#[derive(Accounts)]
pub struct WithdrawSol<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(seeds=[b"v2-project".as_ref(),&project.id],bump=project.bump,has_one=creator)]
    pub project: Box<Account<'info, Project>>,
    #[account(mut,seeds=[b"v2-payment".as_ref(),project.key().as_ref(),&[0]],bump=ledger.bump,has_one=project,constraint=ledger.currency==0)]
    pub ledger: Box<Account<'info, PaymentLedger>>,
    /// CHECK: locked fee recipient; only recorded income is transferred.
    #[account(mut,address=project.treasury)]
    pub treasury: UncheckedAccount<'info>,
}
#[event_cpi]
#[derive(Accounts)]
#[instruction(currency:u8)]
pub struct WithdrawToken<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(seeds=[b"v2-project".as_ref(),&project.id],bump=project.bump,has_one=creator)]
    pub project: Box<Account<'info, Project>>,
    #[account(mut,seeds=[b"v2-payment".as_ref(),project.key().as_ref(),&[currency]],bump=ledger.bump,has_one=project,constraint=ledger.currency==currency)]
    pub ledger: Box<Account<'info, PaymentLedger>>,
    /// CHECK: locked fee recipient.
    #[account(address=project.treasury)]
    pub treasury: UncheckedAccount<'info>,

    #[account(constraint=(currency==1 && payment_mint.key()==project.usdc_mint)||(currency==2 && payment_mint.key()==project.usdt_mint),mint::decimals=6,mint::token_program=payment_program)]
    pub payment_mint: Box<InterfaceAccount<'info, Mint>>,
    pub payment_program: Program<'info, anchor_spl::token::Token>,
    #[account(mut,associated_token::mint=payment_mint,associated_token::authority=ledger,associated_token::token_program=payment_program)]
    pub payment_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(init_if_needed,payer=creator,associated_token::mint=payment_mint,associated_token::authority=creator,associated_token::token_program=payment_program)]
    pub creator_tokens: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(init_if_needed,payer=creator,associated_token::mint=payment_mint,associated_token::authority=treasury,associated_token::token_program=payment_program)]
    pub treasury_tokens: Box<InterfaceAccount<'info, TokenAccount>>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
#[event_cpi]
#[derive(Accounts)]
pub struct ReturnTokens<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(mut,seeds=[b"v2-project".as_ref(),&project.id],bump=project.bump,has_one=creator)]
    pub project: Box<Account<'info, Project>>,
    #[account(address=project.mint,mint::token_program=token_program)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut,associated_token::mint=mint,associated_token::authority=project,associated_token::token_program=token_program)]
    pub token_vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(init_if_needed,payer=creator,associated_token::mint=mint,associated_token::authority=creator,associated_token::token_program=token_program)]
    pub creator_tokens: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(address=project.token_program)]
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    #[account(seeds=[b"v2-payment".as_ref(),project.key().as_ref(),&[0]],bump=sol_ledger.bump,has_one=project,constraint=sol_ledger.currency==0)]
    pub sol_ledger: Box<Account<'info, PaymentLedger>>,
    #[account(seeds=[b"v2-payment".as_ref(),project.key().as_ref(),&[1]],bump=usdc_ledger.bump,has_one=project,constraint=usdc_ledger.currency==1)]
    pub usdc_ledger: Box<Account<'info, PaymentLedger>>,
    #[account(seeds=[b"v2-payment".as_ref(),project.key().as_ref(),&[2]],bump=usdt_ledger.bump,has_one=project,constraint=usdt_ledger.currency==2)]
    pub usdt_ledger: Box<Account<'info, PaymentLedger>>,
    pub system_program: Program<'info, System>,
}
