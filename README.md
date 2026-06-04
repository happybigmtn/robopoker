# robopoker

[![license](https://img.shields.io/github/license/krukah/robopoker)](LICENSE)
[![build](https://github.com/krukah/robopoker/actions/workflows/ci.yml/badge.svg)](https://github.com/krukah/robopoker/actions/workflows/ci.yml)

A Rust toolkit for game-theoretically optimal poker strategies, implementing state-of-the-art algorithms for No-Limit Texas Hold'em with functional parity to Pluribus<sup>1</sup>.

## Visual Tour

<table align="center">
<tr>
<td align="center">
    <img src="https://github.com/user-attachments/assets/5118eba3-3d64-42f8-ac07-5c83ff733439" height="200" alt="Training Progress"/>
    <br>
    <em>Monte Carlo Tree Search</em>
</td>
<td align="center">
    <img src="https://github.com/user-attachments/assets/90b491df-9482-483e-9475-4360f5a17add" height="200" alt="Strategy Growth"/>
    <br>
    <em>Equity Distributions</em>
</td>
</tr>
</table>

## Features

- **Fastest open-source hand evaluator** - Nanosecond evaluation outperforming Cactus Kev
- **Strategic abstraction** - Hierarchical k-means clustering of 3.1T poker situations
- **Optimal transport** - Earth Mover's Distance via Sinkhorn algorithm
- **MCCFR solver** - External sampling with dynamic tree construction
- **PostgreSQL persistence** - Binary format serialization for efficiency
- **Short deck support** - 36-card variant with adjusted rankings

## Quick Start

Add robopoker to your `Cargo.toml`:

```toml
[dependencies]
rbp = "1.0"

# Or individual crates:
rbp-cards = "1.0"
rbp-gameplay = "1.0"
rbp-mccfr = "1.0"
```

### Basic Usage

```rust
use rbp::cards::*;
use rbp::gameplay::*;

// Create a hand and evaluate it
let hand = Hand::from("AcKsQhJdTc9h8s");
let strength = hand.evaluate();

// Work with observations
let obs = Observation::from(Street::Flop);
let equity = obs.equity();
```

## Crate Overview

| Crate | Description |
|-------|-------------|
| [`rbp`](crates/rbp) | Facade re-exporting all public crates |
| [`rbp-core`](crates/util) | Type aliases, constants, DTOs, shared traits |
| [`rbp-cards`](crates/cards) | Card primitives, hand evaluation, equity |
| [`rbp-transport`](crates/transport) | Optimal transport (Sinkhorn, EMD) |
| [`rbp-mccfr`](crates/mccfr) | Game-agnostic CFR framework |
| [`rbp-gameplay`](crates/gameplay) | Poker game engine |
| [`rbp-clustering`](crates/clustering) | K-means abstraction |
| [`rbp-nlhe`](crates/nlhe) | No-Limit Hold'em solver |
| [`rbp-database`](crates/database) | PostgreSQL persistence layer |
| [`rbp-auth`](crates/auth) | JWT + Argon2 authentication |
| [`rbp-gameroom`](crates/gameroom) | Async game coordinator, players, hand history |
| [`rbp-server`](crates/server) | Unified HTTP server (analysis API + game hosting) |
| [`rbp-autotrain`](crates/autotrain) | Training orchestration with distributed workers |

## Architecture

### Core Layer

**`rbp-cards`** — Card representation, hand evaluation, and strategic primitives:
- Bijective card representations (`u8`/`u16`/`u32`/`u64`) for efficient operations
- Lazy hand strength evaluation in nanoseconds
- Equity calculation via enumeration and Monte Carlo
- Exhaustive iteration over cards, hands, decks, and observations
- Short deck (36-card) variant support

**`rbp-transport`** — Optimal transport algorithms:
- Sinkhorn iteration for near-linear Wasserstein approximation<sup>5</sup>
- Greenhorn optimization for sparse distributions
- Generic `Measure` abstraction for arbitrary metric spaces

**`rbp-mccfr`** — Game-agnostic CFR framework:
- State primitives: `Turn`, `Edge`, `Game`, `Info`, `Tree`
- Strategy representation: `Encoder`, `Profile`, `InfoSet`
- Training: `Solver` trait with pluggable algorithms
- Schemes: `RegretSchedule`, `PolicySchedule`, `SamplingScheme`
- Subgame solving with safe search

### Domain Layer

**`rbp-gameplay`** — Complete poker game engine:
- Full No-Limit Texas Hold'em rules
- Complex showdown handling (side pots, all-ins, ties)
- Bet sizing abstraction via `Size` enum (`SPR(n,d)` / `BBs(n)`)
- Clean Node/Edge/Tree game state representation

**`rbp-clustering`** — Hand abstraction via clustering:
- Hierarchical k-means with Elkan acceleration
- Earth Mover's Distance between distributions
- Isomorphic exhaustion of 3.1T situations<sup>4</sup>
- PostgreSQL binary persistence

**`rbp-nlhe`** — Concrete NLHE solver:
- `NlheSolver<R, W, S>` with pluggable regret/policy/sampling
- `NlheEncoder` for state→info mapping
- `NlheProfile` for regret/strategy storage
- Production config: `Flagship` type alias

### Infrastructure Layer

**`rbp-database`** — PostgreSQL persistence:
- Binary format serialization for efficient storage
- Schema definitions and streaming I/O via `COPY IN` protocol
- `Source` trait for SELECT, `Sink` trait for INSERT/UPDATE
- Training stage tracking and validation

**`rbp-gameroom`** — Async game coordination:
- Room-based multiplayer game management
- Pluggable player implementations (AI, human, network)
- Hand history recording and replay

**`rbp-server`** — Unified HTTP server:
- Analysis API for querying training results
- Game hosting with WebSocket support
- Authentication integration

**`rbp-autotrain`** — Training orchestration:
- Two-phase: clustering then MCCFR
- Fast (in-memory) and slow (distributed) modes
- Graceful interrupts and resumable state
- Timed training via `TRAIN_DURATION`

## Training Pipeline

1. **Hierarchical Abstraction** (per street: river → turn → flop → preflop):
   - Generate isomorphic hand clusters
   - Initialize k-means centroids via k-means++<sup>2</sup>
   - Run clustering to group strategically similar hands
   - Calculate EMD metrics via optimal transport<sup>5</sup>
   - Save abstractions to PostgreSQL

2. **MCCFR Training**<sup>3</sup>:
   - Sample game trajectories via external sampling
   - Update regret values and counterfactual values
   - Accumulate strategy with linear weighting
   - Checkpoint blueprint strategy to database

3. **Real-time Search** (in progress):
   - Depth-limited subgame solving<sup>10</sup>
   - Blueprint strategy as prior
   - Targeted Monte Carlo rollouts

## System Requirements

| Street  | Abstraction Size | Metric Size |
| ------- | ---------------- | ----------- |
| Preflop | 4 KB             | 301 KB      |
| Flop    | 32 MB            | 175 KB      |
| Turn    | 347 MB           | 175 KB      |
| River   | 3.02 GB          | -           |

**Recommended:**
- Training: 16 vCPU, 120GB RAM
- Database: PostgreSQL 14+ with 8 vCPU, 64GB RAM
- Analysis: 1 vCPU, 4GB RAM

## Feature Flags

| Feature | Description |
|---------|-------------|
| `database` | PostgreSQL integration |
| `server` | Server dependencies (Actix, Tokio, Rayon) |
| `shortdeck` | 36-card short deck variant |

## Building

```bash
# Build all crates
cargo build --workspace

# Build with database features
cargo build --workspace --features database

# Run tests
cargo test --workspace

# Generate documentation
cargo doc --workspace --no-deps --open
```

## TUI Preview

`robopoker-tui` is a read-only Ratatui dashboard for local inspection of the
evaluator and training posture. It does not connect to the server, database, or
any live play path. The interactive view advances one poker beat at a time:
`Space` / `Enter` steps forward, `b` / `Backspace` steps back, `r` loads a new
seeded hand, and TachyonFX marks each transition.

```bash
cargo run -p robopoker-tui
cargo run -p robopoker-tui -- --headless --seed 49363 --export-dir .auto/tui
cargo test -p robopoker-tui
```

## Testnet launch proof

The end-to-end testnet launch chain
(`--cluster` → `--reset` → `--smoke` → `--status` → `--bench` →
`--compare` → `--replay <transcript>`) is wrapped in a single
operator-visible runbook:

```bash
DATABASE_URL=postgres://user:***@host:5432/dbname \
    bash scripts/testnet-live-proof.sh
```

The runbook drives the `trainer` binary as a subprocess against one
Postgres, captures each step's stdout + stderr + exit code into a
per-step receipt under `receipts/testnet-live-proof-<UTC-ISO>/`, and
emits a one-line `testnet live_proof complete: smoke=N status=N
bench=N compare=N replay=BYTES` headline in `SUMMARY.txt` a testnet
dashboard can scrape. See
[`scripts/testnet-live-proof.md`](scripts/testnet-live-proof.md) for
the full runbook, the receipt layout, the env knobs honoured, and
the shell-shape pinner (`cargo test -p rbp-autotrain --test
script_shape`) that catches a runbook regression without needing a
live database.

## Testnet publish bundle

A v7 follow-on the runbook names as the "next slice" turns the
local receipt into a portable bundle a third party (a testnet
dashboard bucket, a CI auditor, a release-gate script) can fetch
+ re-verify without re-running the chain:

```bash
bash scripts/testnet-live-publish.sh \
    receipts/testnet-live-proof-20260604T050000Z/
```

The runbook chains `trainer --verify-receipt <receipt>` (a
pre-publish gate that refuses to publish a red receipt) and
`trainer --publish <receipt>` (a deterministic, content-addressed
bundle writer) as subprocesses, and drops a
`publish/testnet-live-proof-<UTC-ISO>/` directory:

```
publish/testnet-live-proof-20260604T050000Z/
  bundle.tar.gz       # deterministic tar.gz of the receipt
  bundle.sha256       # single-line sha256 of the tarball
  manifest.json       # per-file sha256 + metadata
  SUMMARY.txt         # the one-line publish headline
```

A downstream auditor can re-verify the bundle with a single
static `trainer` binary (no Postgres required):

```bash
trainer --verify-bundle publish/testnet-live-proof-20260604T050000Z/
# live_proof bundle verification passed: bundle=bundle.tar.gz files=24 bytes=18967 sha256=...
```

The publish step is **read-only** with respect to the receipt
(it copies the receipt into a fresh `staging/` tempdir before
tarring) and does **not** push to a remote registry — the
operator (or a CI worker) can `aws s3 cp` /
`gsutil cp` the local `publish/<basename>/` directory into a
testnet dashboard bucket in a follow-on slice. The committed
no-DB reference bundle at
[`crates/autotrain/tests/fixtures/publish-fixture/`](crates/autotrain/tests/fixtures/publish-fixture/)
is re-verified on every `cargo test --workspace` run. See
[`scripts/testnet-live-publish.md`](scripts/testnet-live-publish.md)
for the full runbook.

## Public dashboard

The v10 follow-on the receipt chain defers to: a static testnet
dashboard a stranger can `curl` + render. The dashboard is a
new `crates/dashboard/` workspace member with three layers:

1. a typed `IndexClient` that re-uses
   `rbp_autotrain::PublishIndex` (so a shape drift in
   `INDEX.json` fails BOTH the dashboard's typed read AND the
   `trainer --verify-index` re-verify at the same CI step);
2. a thin `axum` router at `GET /` (serves a static
   `index.html`) + `GET /api/index` (returns the typed
   `INDEX.json`) + `GET /transcript/:id` (proxies the
   `transcript-<id>.json` bundle) + `GET /bench/:id` (renders a
   `BenchReport` as a card);
3. a static vanilla-JS `index.html` (no framework; no build
   step; no `npm`) that fetches `/api/index` and renders a
   sortable table of receipts with columns for
   `receipt_basename` / `blueprint` / `baseline` / `mbb_per_100`
   / `ci_95` / `win_rate` / `total_bytes` / `uploaded_at_utc`, a
   per-row `Download transcript` link to `/transcript/:id`,
   and a per-row `Open replay` link to `/bench/:id`.

Public dashboard: <https://robopoker-testnet-dashboard.pages.dev/>

The `RBP_DASHBOARD_DEPLOYED_URL` env knob the test harness
sets; the placeholder URL the v10 ships with is greppable so a
dashboard-readiness check can `grep -q 'Public dashboard:'
README.md`. A testnet dashboard can `curl
https://robopoker-testnet-dashboard.pages.dev/api/index` and
receive the same `INDEX.json` shape the `trainer --verify-index`
re-verifier accepts. The deploy runbook is

```bash
bash scripts/testnet-live-publish-dashboard.sh \
    receipts/publish-20260604T050000Z/ s3://robopoker-testnet-dashboard
```

which chains `trainer --verify-index <index-dir>` (the
pre-deploy refuse-to-deploy-red-index gate) and
`aws s3 sync <publish-root>/index/ s3://<bucket>/<prefix>/ --delete
--cache-control max-age=60` as a sequence of subprocesses.
See [`scripts/testnet-live-publish-dashboard.md`](scripts/testnet-live-publish-dashboard.md)
for the full runbook.

## Workspace parallel test proof

The full `cargo test --workspace -- --test-threads=4` chain is
wrapped in a single operator-visible runbook that proves the
workspace stays green across 3 consecutive parallel runs:

```bash
bash scripts/workspace-parallel-proof.sh
```

The runbook runs `cargo test --workspace -- --test-threads=4` 3
times back-to-back, captures each run's stdout + stderr + exit
code into a per-run directory under
`logs/workspace-parallel-proof/<UTC-ISO>/run-{1,2,3}/`, and emits
a one-line `workspace parallel proof complete: runs=3 failures=0`
headline in `SUMMARY.txt` on success. Knobs (all optional):
`RBP_WORKSPACE_PARALLEL_THREADS` (default `4`),
`RBP_WORKSPACE_PARALLEL_RUNS` (default `3`),
`RBP_WORKSPACE_PARALLEL_SKIP_BUILD` (set to `1` to skip the
one-time pre-build for tight inner loops). Exit codes: `0` all
runs passed, `1` script-internal error, `3` one or more runs
failed. The shell-shape pinner (`cargo test -p rbp-autotrain
--test workspace_parallel_proof`) catches a runbook regression
(script missing, `bash -n` broken, executable bit cleared,
SUMMARY.txt headline format drift) without needing 3 full
workspace runs.

## References

1. (2019). Superhuman AI for multiplayer poker. [(Science)](https://science.sciencemag.org/content/early/2019/07/10/science.aay2400)
2. (2014). Potential-Aware Imperfect-Recall Abstraction with Earth Mover's Distance in Imperfect-Information Games. [(AAAI)](http://www.cs.cmu.edu/~sandholm/potential-aware_imperfect-recall.aaai14.pdf)
3. (2007). Regret Minimization in Games with Incomplete Information. [(NIPS)](https://papers.nips.cc/paper/3306-regret-minimization-in-games-with-incomplete-information)
4. (2013). A Fast and Optimal Hand Isomorphism Algorithm. [(AAAI)](https://www.cs.cmu.edu/~waugh/publications/isomorphism13.pdf)
5. (2018). Near-linear time approximation algorithms for optimal transport via Sinkhorn iteration. [(NIPS)](https://arxiv.org/abs/1705.09634)
6. (2019). Solving Imperfect-Information Games via Discounted Regret Minimization. [(AAAI)](https://arxiv.org/pdf/1809.04040.pdf)
7. (2013). Action Translation in Extensive-Form Games with Large Action Spaces. [(IJCAI)](http://www.cs.cmu.edu/~sandholm/reverse%20mapping.ijcai13.pdf)
8. (2015). Discretization of Continuous Action Spaces in Extensive-Form Games. [(AAMAS)](http://www.cs.cmu.edu/~sandholm/discretization.aamas15.fromACM.pdf)
9. (2015). Regret-Based Pruning in Extensive-Form Games. [(NIPS)](http://www.cs.cmu.edu/~sandholm/regret-basedPruning.nips15.withAppendix.pdf)
10. (2018). Depth-Limited Solving for Imperfect-Information Games. [(NeurIPS)](https://arxiv.org/pdf/1805.08195.pdf)
11. (2017). Reduced Space and Faster Convergence in Imperfect-Information Games via Pruning. [(ICML)](http://www.cs.cmu.edu/~sandholm/reducedSpace.icml17.pdf)
12. (2017). Safe and Nested Subgame Solving for Imperfect-Information Games. [(NIPS)](https://www.cs.cmu.edu/~noamb/papers/17-NIPS-Safe.pdf)

## License

MIT License - see [LICENSE](LICENSE) for details.
