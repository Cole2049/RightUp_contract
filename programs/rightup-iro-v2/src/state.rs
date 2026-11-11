use anchor_lang::prelude::*;

/// Longest token, collection or allocation metadata URI, in bytes. With a
/// 32-byte name and a 10-byte symbol three URIs of this length make the
/// three-signer preparation transaction 1144 bytes of the 1232 a packet holds;
/// `ipfs://` plus a base32 CIDv1 is 66. The `max_len` attributes on `Project`
/// below repeat this number.
pub const MAX_URI_LEN: usize = 100;

/// Seed of an allocation's Core asset: `[ASSET_SEED, project, index LE]`. The
/// address alone ties an asset to its index; the program signs its creation.
pub const ASSET_SEED: &[u8] = b"v2-asset";

/// Bytes of the claim bitmap for an allocation count.
pub fn claim_bitmap_len(allocation_count: u32) -> usize {
    (allocation_count as usize).div_ceil(8)
}

#[account]
#[derive(InitSpace)]
pub struct Platform {
    pub admin: Pubkey,
    /// Signs price references and co-signs every project preparation.
    pub price_authority: Pubkey,
    pub draw_authority: Pubkey,
    pub treasury: Pubkey,
    pub genesis_hash: [u8; 32],
    pub usdc_mint: Pubkey,
    pub usdt_mint: Pubkey,
    pub head: u64,
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, InitSpace)]
pub struct Terms {
    pub multiplier_bps: u32,
    pub user_fee_bps: u16,
    pub issuer_fee_bps: u16,
    pub participation_seconds: u32,
    pub exercise_seconds: u32,
    pub allocation_count: u32,
    pub digest: [u8; 32],
}

#[account]
#[derive(InitSpace)]
pub struct ConfigVersion {
    pub version: u64,
    pub terms: Terms,
}

#[account]
#[derive(InitSpace)]
pub struct Project {
    pub id: [u8; 16],
    pub creator: Pubkey,
    pub mint: Pubkey,
    pub token_program: Pubkey,
    pub collection: Pubkey,
    /// Zero until launch; terms are the head version at launch time.
    pub config_version: u64,
    /// Copied from the platform at launch; later rotation never moves a published project.
    pub treasury: Pubkey,
    pub usdc_mint: Pubkey,
    pub usdt_mint: Pubkey,
    pub terms: Terms,
    pub token_decimals: u8,
    pub total_supply: u64,
    pub first_buy_tokens: u64,
    pub first_buy_cost: u64,
    pub quote_mint: Pubkey,
    pub escrow_amount: u64,
    pub tokens_per_allocation: u64,
    pub base_amounts: [u64; 3],
    pub snapshot_hash: [u8; 32],
    pub published_at: i64,
    pub participation_ends_at: i64,
    /// Last moment the draw root may be committed.
    pub draw_deadline: i64,
    /// Zero until the draw root is committed; then commit time plus exercise_seconds.
    pub exercise_ends_at: i64,
    pub draw_id: [u8; 16],
    pub draw_root: [u8; 32],
    pub allocated_count: u32,
    pub draw_committed: bool,
    pub remaining_returned: bool,
    pub bump: u8,
    #[max_len(64)]
    pub nft_name: String,
    #[max_len(10)]
    pub symbol: String,
    #[max_len(100)]
    pub token_uri: String,
    pub holder_rewards: bool,
    pub creator_fee_bps: u64,
    #[max_len(100)]
    pub nft_uri: String,
    /// One bit per allocation index, set when it is claimed. Sized at launch to
    /// the locked allocation count; a claimed index can never be claimed again.
    #[max_len(0)]
    pub claimed: Vec<u8>,
}

#[account]
#[derive(InitSpace)]
pub struct PaymentLedger {
    pub project: Pubkey,
    pub currency: u8,
    pub gross_base: u64,
    pub user_fees: u64,
    pub exercised_count: u32,
    pub withdrawn: bool,
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct PriceReference {
    pub quote_mint: Pubkey,
    pub quote_decimals: u8,
    pub quote_micro_u: u64,
    pub sol_micro_u: u64,
    pub observed_at: i64,
    pub expires_at: i64,
    pub nonce: [u8; 32],
    pub source_hash: [u8; 32],
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct PrepareArgs {
    pub id: [u8; 16],
    pub name: String,
    pub symbol: String,
    pub token_uri: String,
    pub collection_uri: String,
    pub nft_uri: String,
    pub quote_mint: Pubkey,
    pub holder_rewards: bool,
    pub creator_fee_bps: u64,
}
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct LaunchArgs {
    pub first_buy_amount: u64,
    pub max_quote_cost: u64,
    pub create_quote_accounts: u8,
    pub reference: PriceReference,
}
#[event]
pub struct PlatformUpdated {
    pub admin: Pubkey,
    pub price_authority: Pubkey,
    pub draw_authority: Pubkey,
    pub treasury: Pubkey,
}
#[event]
pub struct ProjectPrepared {
    pub project: Pubkey,
    pub id: [u8; 16],
    pub creator: Pubkey,
    pub mint: Pubkey,
    pub collection: Pubkey,
    pub quote_mint: Pubkey,
    pub holder_rewards: bool,
    pub creator_fee_bps: u64,
    pub name_hash: [u8; 32],
    pub symbol_hash: [u8; 32],
    pub token_uri_hash: [u8; 32],
    pub collection_uri_hash: [u8; 32],
    pub nft_uri_hash: [u8; 32],
}

#[event]
pub struct ConfigVersionCreated {
    pub version: u64,
    pub terms: Terms,
}
#[event]
pub struct ProjectPublished {
    pub project: Pubkey,
    pub id: [u8; 16],
    pub creator: Pubkey,
    pub mint: Pubkey,
    pub collection: Pubkey,
    pub config_version: u64,
    pub snapshot_hash: [u8; 32],
    pub total_supply: u64,
    pub first_buy_tokens: u64,
    pub first_buy_cost: u64,
    pub quote_mint: Pubkey,
    pub escrow_amount: u64,
    pub tokens_per_allocation: u64,
    pub allocation_count: u32,
    pub base_amounts: [u64; 3],
    pub published_at: i64,
    pub participation_ends_at: i64,
    pub draw_deadline: i64,
    pub quote_micro_u: u64,
    pub sol_micro_u: u64,
    pub observed_at: i64,
    pub expires_at: i64,
}
#[event]
pub struct DrawCommitted {
    pub project: Pubkey,
    pub draw_id: [u8; 16],
    pub root: [u8; 32],
    pub allocated_count: u32,
    pub exercise_ends_at: i64,
}
#[event]
pub struct AllocationClaimed {
    pub project: Pubkey,
    pub index: u32,
    pub asset: Pubkey,
    pub claim_wallet: Pubkey,
}
#[event]
pub struct AllocationExercised {
    pub project: Pubkey,
    pub index: u32,
    pub asset: Pubkey,
    pub holder: Pubkey,
    pub currency: u8,
    pub base_amount: u64,
    pub user_fee: u64,
    pub token_amount: u64,
}
#[event]
pub struct IncomeWithdrawn {
    pub project: Pubkey,
    pub currency: u8,
    pub gross_base: u64,
    pub issuer_fee: u64,
    pub net: u64,
}
#[event]
pub struct RemainingTokensReturned {
    pub project: Pubkey,
    pub amount: u64,
}

#[error_code]
pub enum IroError {
    #[msg("Invalid authority")]
    Authority,
    #[msg("Invalid account or protocol")]
    Account,
    #[msg("Invalid immutable terms")]
    Terms,
    #[msg("Arithmetic overflow or zero amount")]
    Arithmetic,
    #[msg("Invalid or expired price reference")]
    Reference,
    #[msg("First buy must cover at least ten percent of total supply")]
    FirstBuy,
    #[msg("Outside allowed period")]
    Period,
    #[msg("Invalid allocation proof")]
    Proof,
    #[msg("Already completed")]
    Completed,
    #[msg("Invalid current Core owner or collection")]
    Holder,
    #[msg("Invalid currency")]
    Currency,
}
