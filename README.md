# RightUp IRO Program

The Solana program behind RightUp, a meme launchpad with an IRO (Initial Right Offering). A creator launches a token on Pump.fun through this program; part of the supply is escrowed for the IRO; winners of a weighted lottery receive a transferable Metaplex Core NFT for each allocation, and whoever holds that NFT can exercise it at a price fixed at launch.

Built with Anchor 1.2.0 (Rust). Program ID: `26tAP5skAJsT49DM4FVC4ivmWy9oFiRRvuBd559UNg7W`.

## Lifecycle

1. **Prepare**: the creator creates the project's Core Collection and the project and payment ledger accounts. The platform's publish authority co-signs, so a project exists only for a definition the platform built.
2. **Launch**: one transaction creates the Pump.fun token and makes the creator's first buy of at least 10% of supply. 5% of total supply goes into a vault owned by the project; the rest stays with the creator. The terms in force at that moment (price multiplier, fees, participation and exercise periods, number of allocations) are locked into the project, together with exercise prices in SOL, USDC and USDT derived from the actual first-buy cost and a price reference signed by the platform (verified with the Ed25519 instruction).
3. **Draw**: after the participation period the draw authority commits the lottery result as a Merkle root, once. The exercise window starts at that moment.
4. **Claim**: each winner claims an allocation with a Merkle proof; the program creates a Core NFT at an address derived from the project and allocation index, and a bitmap prevents a second claim.
5. **Exercise**: the current NFT holder (not necessarily the original winner) pays the locked base amount plus the user fee in SOL, USDC or USDT. Payment, NFT burn and token release happen atomically; several allocations can be exercised separately.
6. **Settle**: when the window ends (or the draw deadline passes with no draw) the creator withdraws the base amounts per currency minus the issuer fee, and takes back every escrowed token that was not exercised.

All amounts are integers in each asset's smallest unit. Prices are computed as one exact rational with checked 256-bit arithmetic and rounded up once.

## Instructions

| Instruction | Signer | Purpose |
|---|---|---|
| `initialize_platform` | program upgrade authority | Create the platform account with its authorities, treasury and stablecoin mints |
| `update_platform` | platform admin | Rotate the price, draw and treasury keys |
| `create_config_version` | platform admin | Publish an immutable set of default terms and move the head to it |
| `prepare_pump_iro` | creator + publish authority | Fix a project's definition and create its Collection and ledgers |
| `create_pump_iro` | creator (+ signed price reference) | Atomic Pump.fun create and first buy, escrow and locked prices |
| `commit_draw` | draw authority | Commit the lottery root and allocation count |
| `claim_allocation` | winner | Verify the Merkle proof and mint the allocation's Core NFT |
| `exercise_sol` / `exercise_token` | current NFT holder | Pay in SOL or USDC/USDT, burn the NFT, receive the tokens |
| `withdraw_sol` / `withdraw_token` | creator | Withdraw income per currency after the window, minus the issuer fee |
| `return_remaining_tokens` | creator | Take back the unexercised escrow after the window |

Events are emitted through Anchor `event-cpi`, so they live in inner instructions and are not lost when transaction logs are truncated.

## Accounts

| Account | Seeds | Content |
|---|---|---|
| Platform | `v2-platform` | Admin, price/publish authority, draw authority, treasury, stablecoin mints, config head |
| ConfigVersion | `v2-config` + version | Immutable default terms |
| Project | `v2-project` + project id | Definition, locked terms and prices, draw result, claim bitmap |
| PaymentLedger | `v2-payment` + project + currency | Base amounts and user fees collected in one currency |
| Core asset | `v2-asset` + project + index | The allocation NFT itself |

## Source layout

| File | Content |
|---|---|
| `lib.rs` | Instruction entry points |
| `contexts.rs` | Account constraints |
| `lifecycle.rs` | Instruction logic |
| `state.rs` | Accounts, arguments, events, errors |
| `pump.rs` | Pump.fun CPI and first-buy accounting |
| `core_adapter.rs` | Metaplex Core CPI |
| `math.rs` | Price and fee arithmetic |
| `proof.rs` | Price reference, snapshot and Merkle verification |

## Build

Toolchain: Anchor CLI 1.2.0, Solana CLI 4.1.2, Rust 1.89 or later.

```sh
cargo fmt --check
cargo check --locked
anchor build
```

The program keypair is not part of this repository. `anchor build` creates a new one under `target/deploy/`; to build and test under your own program ID, run `anchor keys sync`. The default cluster in `Anchor.toml` is localnet, and nothing here deploys.

## Status

Deployed to Solana devnet for testing. Not yet deployed to mainnet and not externally audited.
