# PieceWork — milestone escrow for freelance work on Tari Ootle

**PieceWork** is a milestone-escrow agreement for freelance work, built on the
Tari Ootle engine. A client creates an agreement naming a payee (the
freelancer) and a list of milestone amounts, and funds it in TARI. For each
milestone, the payee marks it delivered and the client approves it — approval
releases exactly that milestone's slice from the escrow vault to the payee.
Money that was never released becomes refundable to the client once the
agreement's deadline (an epoch) has passed. Either party can cancel the
agreement while it is still unfunded.

It is the escrow pattern every freelance marketplace reimplements with a
trusted middleman and a percentage fee — here the template is the middleman,
and its rules are enforced by the engine, in the open.

## What's in this repo

| Path | What it is |
| --- | --- |
| `src/lib.rs` | The PieceWork Ootle template (Rust, `tari_template_lib` 0.41). One component, one vault, no external dependencies. |
| `tests/piecework.rs` | Engine tests (`tari_template_test_tooling` 0.54) that run the real Ootle engine in-process and exercise every rule below. |
| `docs/index.html` | A walletless browser simulation of the exact same flow and rules — the quickest way to try PieceWork. Dependency-free static page; also served via GitHub Pages. |

## The rules the template enforces

- The transaction signer who creates the agreement is recorded as the **payer**;
  the payee is a public key fixed at creation and cannot be changed.
- **Only the payer** can fund, approve and refund. **Only the payee** can mark
  a milestone delivered. Every state-changing method checks the transaction
  signer's public key itself; the component's access rules are locked with
  `OwnerRule::None`, so nobody can rewrite them later.
- Funding is capped at the total milestone amount — the escrow can never hold
  more than the agreed price.
- A milestone can be released only after the payee marked it delivered, only
  once, and only if the vault actually holds the funds: **no release without
  funding, no double release**.
- **Refunds** return the entire unreleased remainder to the payer, only at or
  after the deadline epoch, and close the agreement.
- **Cancellation** is open to either party, but only while the agreement is
  completely unfunded.

## Run the tests

```sh
cargo test
```

The suite covers: exact one-slice release on fund + deliver + approve; a
stranger cannot approve; the payee cannot self-approve; a non-payee cannot
mark delivery; approval before delivery fails; double approval fails; an
unfunded agreement cannot release; refund before the deadline fails and after
the deadline returns exactly the unreleased remainder; cancellation rules
(either party while unfunded, never once funded, never by a stranger).

## Try the MVP

Open `docs/index.html` in a browser (or the GitHub Pages deployment of this
repo). Create an agreement, fund it, switch the acting role between client,
payee and stranger, and watch invalid actions get rejected with the same
messages the template asserts on-chain.

## Honesty notes

- The **simulation is the judge path**: it mirrors the template's rules one
  for one, runs entirely in the browser, and needs no wallet, faucet or
  testnet. It uses pretend balances — no real TARI moves.
- The **template has not been published** to Esmeralda or mainnet. The
  on-chain artifact in this submission is the template source plus its green
  engine tests, not a deployed component.
- Amounts in the template are micro-TARI (µT). Deadlines are expressed in
  epochs, matching the engine's `Consensus::current_epoch()`.
- PieceWork is new code written for the Tari November Build Contest (theme:
  MVP). It does not borrow code from other entries.

## License

MIT — see `LICENSE`.
