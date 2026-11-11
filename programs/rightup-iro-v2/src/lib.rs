use anchor_lang::prelude::*;

mod contexts;
mod core_adapter;
mod lifecycle;
mod math;
mod proof;
mod pump;
mod state;
pub use contexts::*;
pub use state::*;

declare_id!("26tAP5skAJsT49DM4FVC4ivmWy9oFiRRvuBd559UNg7W");

#[program]
pub mod rightup_iro_v2 {
    use super::*;
    pub fn initialize_platform(
        ctx: Context<InitializePlatform>,
        price_authority: Pubkey,
        draw_authority: Pubkey,
        treasury: Pubkey,
        genesis_hash: [u8; 32],
    ) -> Result<()> {
        lifecycle::initialize(ctx, price_authority, draw_authority, treasury, genesis_hash)
    }
    pub fn update_platform(
        ctx: Context<UpdatePlatform>,
        price_authority: Pubkey,
        draw_authority: Pubkey,
        treasury: Pubkey,
    ) -> Result<()> {
        lifecycle::update_platform(ctx, price_authority, draw_authority, treasury)
    }
    pub fn create_config_version(
        ctx: Context<CreateConfig>,
        version: u64,
        terms: Terms,
    ) -> Result<()> {
        lifecycle::configure(ctx, version, terms)
    }
    pub fn prepare_pump_iro(ctx: Context<PreparePump>, args: PrepareArgs) -> Result<()> {
        lifecycle::prepare(ctx, args)
    }
    pub fn create_pump_iro<'info>(
        ctx: Context<'info, CreatePump<'info>>,
        args: LaunchArgs,
    ) -> Result<()> {
        lifecycle::launch(ctx, args)
    }
    pub fn commit_draw(
        ctx: Context<CommitDraw>,
        draw_id: [u8; 16],
        root: [u8; 32],
        count: u32,
    ) -> Result<()> {
        lifecycle::draw(ctx, draw_id, root, count)
    }
    pub fn claim_allocation(
        ctx: Context<ClaimAllocation>,
        index: u32,
        nodes: Vec<[u8; 32]>,
    ) -> Result<()> {
        lifecycle::claim(ctx, index, nodes)
    }
    pub fn exercise_sol(ctx: Context<ExerciseSol>, index: u32) -> Result<()> {
        lifecycle::exercise_sol(ctx, index)
    }
    pub fn exercise_token(ctx: Context<ExerciseToken>, currency: u8, index: u32) -> Result<()> {
        lifecycle::exercise_token(ctx, currency, index)
    }
    pub fn withdraw_sol(ctx: Context<WithdrawSol>) -> Result<()> {
        lifecycle::withdraw_sol(ctx)
    }
    pub fn withdraw_token(ctx: Context<WithdrawToken>, currency: u8) -> Result<()> {
        lifecycle::withdraw_token(ctx, currency)
    }
    pub fn return_remaining_tokens(ctx: Context<ReturnTokens>) -> Result<()> {
        lifecycle::return_tokens(ctx)
    }
}
