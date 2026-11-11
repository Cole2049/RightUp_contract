use crate::{state::*, ID};
use anchor_lang::prelude::*;
use solana_instructions_sysvar::{load_current_index_checked, load_instruction_at_checked};
use solana_sha256_hasher::hashv;

pub fn reference_message(
    platform: &Platform,
    project_id: &[u8; 16],
    creator: &Pubkey,
    mint: &Pubkey,
    version: u64,
    reference: &PriceReference,
) -> Result<Vec<u8>> {
    let mut message = b"rightup-v2-price-v1".to_vec();
    message.extend_from_slice(&platform.genesis_hash);
    message.extend_from_slice(ID.as_ref());
    message.extend_from_slice(project_id);
    message.extend_from_slice(creator.as_ref());
    message.extend_from_slice(mint.as_ref());
    message.extend_from_slice(&version.to_le_bytes());
    reference.serialize(&mut message)?;
    Ok(message)
}
pub fn verify_reference(
    sysvar: &AccountInfo,
    signer: &Pubkey,
    message: &[u8],
    reference: &PriceReference,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;
    require!(
        reference.observed_at <= now + 5
            && reference.expires_at > now
            && reference.expires_at > reference.observed_at
            && reference.expires_at - reference.observed_at <= 60
            && reference.quote_micro_u > 0
            && reference.sol_micro_u > 0,
        IroError::Reference
    );
    let index = load_current_index_checked(sysvar)?;
    require!(index > 0, IroError::Reference);
    let ix = load_instruction_at_checked((index - 1) as usize, sysvar)?;
    // Exactly one self-contained Ed25519 signature in the immediately preceding instruction.
    require!(
        ix.program_id == solana_sdk_ids::ed25519_program::ID
            && ix.accounts.is_empty()
            && ix.data.len() >= 16
            && ix.data[0] == 1
            && ix.data[1] == 0,
        IroError::Reference
    );
    let field = |i: usize| u16::from_le_bytes([ix.data[i], ix.data[i + 1]]);
    require!(
        field(4) == u16::MAX && field(8) == u16::MAX && field(14) == u16::MAX,
        IroError::Reference
    );
    let sig = field(2) as usize;
    let key = field(6) as usize;
    let msg = field(10) as usize;
    let len = field(12) as usize;
    require!(
        sig >= 16
            && key >= 16
            && msg >= 16
            && sig.checked_add(64).is_some_and(|n| n <= ix.data.len())
            && len == message.len(),
        IroError::Reference
    );
    require!(
        ix.data.get(key..key + 32) == Some(signer.as_ref())
            && ix.data.get(msg..msg + len) == Some(message),
        IroError::Reference
    );
    Ok(())
}
/// Commits to everything locked at launch, so an allocation leaf cannot be
/// replayed against different prices, terms, amounts or deadlines.
pub fn snapshot(
    message: &[u8],
    version: u64,
    terms: &Terms,
    amounts: [u64; 5],
    base_amounts: &[u64; 3],
    times: [i64; 3],
) -> Result<[u8; 32]> {
    let mut locked = version.to_le_bytes().to_vec();
    terms.serialize(&mut locked)?;
    for v in amounts.iter().chain(base_amounts) {
        locked.extend_from_slice(&v.to_le_bytes());
    }
    for v in times {
        locked.extend_from_slice(&v.to_le_bytes());
    }
    Ok(hashv(&[
        b"rightup-v2-snapshot-v1",
        &hashv(&[message]).to_bytes(),
        &locked,
    ])
    .to_bytes())
}
pub fn allocation_leaf(
    project: &Pubkey,
    index: u32,
    wallet: &Pubkey,
    quota: u64,
    snapshot: &[u8; 32],
) -> [u8; 32] {
    hashv(&[
        b"rightup-v2-allocation-v1",
        ID.as_ref(),
        project.as_ref(),
        &index.to_le_bytes(),
        wallet.as_ref(),
        &quota.to_le_bytes(),
        snapshot,
    ])
    .to_bytes()
}
pub fn verify_merkle(mut leaf: [u8; 32], proof: &[[u8; 32]], root: [u8; 32]) -> Result<()> {
    require!(proof.len() <= 32, IroError::Proof);
    for node in proof {
        leaf = if leaf <= *node {
            hashv(&[&leaf, node]).to_bytes()
        } else {
            hashv(&[node, &leaf]).to_bytes()
        };
    }
    require!(leaf == root, IroError::Proof);
    Ok(())
}
