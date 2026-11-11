use crate::state::IroError;
use anchor_lang::prelude::*;
mod wide {
    uint::construct_uint! { pub struct U256(4); }
}
use wide::U256;

fn mul(a: U256, b: U256) -> Result<U256> {
    a.checked_mul(b).ok_or_else(|| error!(IroError::Arithmetic))
}
fn ceil_u64(n: U256, d: U256) -> Result<u64> {
    require!(!d.is_zero(), IroError::Arithmetic);
    let (q, r) = n.div_mod(d);
    let q = if r.is_zero() {
        q
    } else {
        q.checked_add(U256::one()).ok_or(IroError::Arithmetic)?
    };
    require!(
        q > U256::zero() && q <= U256::from(u64::MAX),
        IroError::Arithmetic
    );
    Ok(q.low_u64())
}
pub fn prices(
    cost: u64,
    bought: u64,
    quota: u64,
    quote_price: u64,
    decimals: u8,
    sol_price: u64,
    multiplier: u32,
) -> Result<[u64; 3]> {
    require!(
        decimals <= 18 && bought > 0 && sol_price > 0,
        IroError::Arithmetic
    );
    let n = mul(
        mul(
            mul(U256::from(cost), U256::from(quote_price))?,
            U256::from(multiplier),
        )?,
        U256::from(quota),
    )?;
    let d = mul(
        mul(U256::from(10u64.pow(decimals.into())), U256::from(bought))?,
        U256::from(10000u64),
    )?;
    let stable = ceil_u64(n, d)?;
    let sol = ceil_u64(
        mul(n, U256::from(1_000_000_000u64))?,
        mul(d, U256::from(sol_price))?,
    )?;
    Ok([sol, stable, stable])
}
pub fn fee(base: u64, bps: u16, round_up: bool) -> Result<u64> {
    let n = (base as u128)
        .checked_mul(bps as u128)
        .ok_or(IroError::Arithmetic)?;
    let v = if round_up {
        n.checked_add(9999).ok_or(IroError::Arithmetic)? / 10000
    } else {
        n / 10000
    };
    u64::try_from(v).map_err(|_| error!(IroError::Arithmetic))
}
