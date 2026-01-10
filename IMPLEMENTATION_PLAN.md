# Terminal Multiplayer Poker with Commonware Integration

## Overview

Build a terminal-based multiplayer poker environment where players can play No-Limit Texas Hold'em over a network with verifiable randomness and distributed consensus.

## Current State Analysis

### Existing Functionality (Complete)

- **Cards** (`src/cards/`): Card/deck/hand/hole/board types, bitwise hand evaluation, equity calculation
- **Gameplay** (`src/gameplay/`): Complete game engine with betting, actions, showdown, settlements, payouts
- **Gameroom** (`src/gameroom/`): `Room` coordinator, `Player` trait, `Actor` wrapper, `Event` broadcasting, `Channel`, `Recall`
- **Hosting** (`src/hosting/`): WebSocket via Actix-web, `Casino` room management, `Client` network player
- **Players** (`src/players/human.rs`): Basic `dialoguer` CLI prompts (currently returns random actions as placeholder)
- **CPU Players** (`src/gameroom/players/fish.rs`): Random action selection via `Player` trait

### Gaps Requiring Implementation

1. **Terminal UI** - No visual rendering exists; need `src/tui/` module with ratatui/crossterm
2. **Commonware RNG** - `src/cards/deck.rs:32` uses `rand::random_range()` - replace with distributed verifiable RNG
3. **Consensus Layer** - `Room` is single authority; need distributed agreement for trustless multiplayer
4. **Variable Player Count** - `src/lib.rs:36` hardcodes `const N: usize = 2`

### Empty/Placeholder Modules

- `src/search/mod.rs` - Empty (reserved for real-time subgame solving)
- `src/players/human.rs` - `decide()` calls `sample()` which returns random action (placeholder)

---

## Priority 1: Terminal UI Module (specs/terminal-ui.md)

**Status**: Not started  
**Blocking**: None  
**Files to Create**: `src/tui/mod.rs`, `src/tui/renderer.rs`, `src/tui/player.rs`, `src/tui/input.rs`

### Tasks

- [ ] Create `src/tui/mod.rs` module structure
- [ ] Add dependencies to Cargo.toml (behind `tui` feature flag):
  ```toml
  ratatui = { optional = true, version = "0.29" }
  crossterm = { optional = true, version = "0.28" }
  ```
- [ ] Implement `TuiRenderer` for game state visualization (minimalist monochrome design):
  - Board/community cards display (use `Card::fmt()` from `src/cards/card.rs`)
  - Player positions around table with chip counts
  - Pot size, blinds, dealer button position
  - Hole cards (private to local player only)
  - Action history/log panel (scrollable)
- [ ] Implement `TuiPlayer` implementing `Player` trait:
  - `async fn decide(&mut self, recall: &Recall) -> Action`
  - `async fn notify(&mut self, event: &Event)`
- [ ] Add keyboard input handling:
  - `F` = Fold, `C` = Check/Call, `R` = Raise
  - Number keys for raise amounts
  - Arrow keys for amount adjustment
- [ ] Support 80x24 minimum terminal size with responsive layout
- [ ] Wire `TuiPlayer` to receive `Event` stream from `Room`

### Integration Points

- Implement `Player` trait from `src/gameroom/player.rs`
- Handle events: `Event::Play`, `Event::ShowHand`, `Event::NextHand`, `Event::YourTurn`
- Reuse `Card`, `Hole`, `Hand` display formatting from `src/cards/`
- Reference `src/players/human.rs` for action selection logic (currently placeholder)

---

## Priority 2: Commonware RNG Integration (specs/commonware-integration.md)

**Status**: Not started  
**Blocking**: None (can develop in parallel with TUI)  
**Files to Create**: `src/rng/mod.rs`, `src/rng/local.rs`, `src/rng/distributed.rs`, `src/rng/protocol.rs`

### Commonware Primitives Available (v0.0.64)

- `commonware-cryptography`: BLS12-381 DKG (`dkg::deal`), threshold signatures, `Sharing`
- `commonware-consensus`: Simplex Byzantine consensus protocol
- `commonware-p2p`: Authenticated encrypted peer communication
- `commonware-runtime`: Async task execution

### Current RNG Usage (to replace)

- `src/cards/deck.rs:32`: `rand::random_range(0..n)` for card selection
- Used in `Deck::draw()`, `Deck::deal()`, `Deck::hole()`

### Tasks

- [ ] Add commonware dependencies to Cargo.toml (behind `commonware` feature):
  ```toml
  commonware-cryptography = { optional = true, version = "0.0.64" }
  commonware-runtime = { optional = true, version = "0.0.64" }
  ```
- [ ] Create `src/rng/mod.rs` with `RngSource` trait:
  ```rust
  pub trait RngSource: Send {
      fn random_u64(&mut self) -> u64;
      fn random_range(&mut self, range: std::ops::Range<usize>) -> usize;
  }
  ```
- [ ] Implement `LocalRng` wrapping `rand::rngs::SmallRng` for tests/single-player
- [ ] Implement commit-reveal protocol in `src/rng/protocol.rs`:
  - `CommitPhase`: Each participant commits `H(entropy || nonce)`
  - `RevealPhase`: Participants reveal entropy after all commits received
  - `CombinePhase`: XOR all revealed entropy to derive seed
- [ ] Implement `DistributedRng` using combined seed from protocol
- [ ] Modify `Deck::draw()` signature: `fn draw(&mut self, rng: &mut impl RngSource) -> Card`
- [ ] Update `Game` to accept `RngSource` for dealing
- [ ] Ensure deterministic shuffle: same seed → same deck order on all nodes

### Verification

- [ ] Unit test: same seed produces identical shuffle
- [ ] Integration test: commit-reveal protocol with 2-4 participants
- [ ] Existing `cargo test` passes with `LocalRng`

---

## Priority 3: Multiplayer Consensus Layer (specs/multiplayer-enhancements.md)

**Status**: Not started  
**Blocking**: Priority 2 (RNG integration) should complete first  
**Files to Create**: `src/consensus/mod.rs`, `src/consensus/state.rs`, `src/consensus/messages.rs`

### Current Architecture (to extend)

- `src/hosting/server.rs`: Actix HTTP/WebSocket server (centralized)
- `src/gameroom/room.rs`: `Room::run()` is single source of truth
- `src/hosting/client.rs`: `Client` player receives JSON over WebSocket

### Recommended: Hybrid Architecture

Keep WebSocket for low-latency messaging; add consensus for critical operations.

### Consensus-Required Operations

1. **Deck shuffling** - RNG seed agreement (from Priority 2)
2. **Action sequencing** - Total ordering of player actions
3. **Showdown settlement** - Pot distribution agreement

### Tasks

- [ ] Add commonware-consensus dependency:
  ```toml
  commonware-consensus = { optional = true, version = "0.0.64" }
  commonware-p2p = { optional = true, version = "0.0.64" }
  ```
- [ ] Create `src/consensus/mod.rs` with `ConsensusLayer` trait
- [ ] Define consensus message types in `src/consensus/messages.rs`:
  - `EntropyCommit { player_id, commitment: Hash }`
  - `EntropyReveal { player_id, entropy: [u8; 32] }`
  - `ActionProposal { action: Action, sequence: u64 }`
  - `ActionAck { sequence: u64, hash: Hash }`
- [ ] Extend WebSocket protocol (`src/hosting/client.rs`):
  - Add `consensus` message type alongside existing `decision`/`event`
- [ ] Implement `ConsensusRoom` wrapping `Room`:
  - Intercept `apply()` to require consensus
  - Compute state hash after each transition
  - Broadcast hash for verification
- [ ] State hash verification:
  - Hash `Game` state deterministically
  - Detect divergence → halt game, report error
- [ ] Handle disconnects: timeout → default action (fold/check)

### Integration Points

- Modify `Room::run()` loop to await consensus before `apply()`
- `Event` broadcast includes state hash for client verification
- `Casino::start()` configures consensus vs non-consensus mode

---

## Priority 4: Variable Player Count & Lobby Enhancements

**Status**: Partially exists  
**Blocking**: Impacts gameplay core—coordinate with other priorities  
**Files to Modify**: `src/lib.rs`, `src/gameplay/game.rs`, `src/hosting/server.rs`

### Current Limitations

- `src/lib.rs:36`: `const N: usize = 2` (compile-time constant)
- `src/hosting/server.rs`: Hardcoded `Room` + `Fish` opponent on `/start`
- No room configuration or listing endpoints

### Tasks

- [ ] Make player count runtime-configurable:
  - Change `const N` to parameter in `Game::new(n: usize, ...)`
  - Update seat/betting logic for N-way pots
- [ ] Add room configuration to `/start` endpoint:
  ```json
  { "max_players": 6, "blinds": [1, 2], "buy_in": 100 }
  ```
- [ ] Implement `/rooms` listing endpoint
- [ ] Add room join by code: `/join/{code}`
- [ ] Configure consensus mode per room (consensus vs trusted server)

---

## Existing TODOs in Codebase

Found via grep search (non-blocking for multiplayer):

| File | Line | Note |
|------|------|------|
| `src/gameplay/game.rs` | 321 | Edge case: all-in blinds |
| `src/cards/observation.rs` | 45 | TODO marker |
| `src/gameplay/recall.rs` | 69 | TODO marker |
| `src/cards/street.rs` | 265, 281 | TODO markers |
| `src/gameplay/abstraction.rs` | 294, 310 | TODO markers |
| `src/mccfr/nlhe/encoder.rs` | 172, 175 | TODO markers |
| `src/analysis/server.rs` | 12 | TODO marker |
| `src/autotrain/epoch.rs` | 28, 31, 34, 37, 40 | Multiple TODOs |

**Assessment**: These are MCCFR/abstraction internals—not blocking for multiplayer poker.

---

## Specifications Status

| Spec | Status | Notes |
|------|--------|-------|
| `specs/terminal-ui.md` | ✓ Complete | Ready for implementation |
| `specs/commonware-integration.md` | ✓ Complete | Ready for implementation |
| `specs/multiplayer-enhancements.md` | ✓ Complete | Ready for implementation |
| `specs/local-solver-bot.md` | ✓ Complete | Lower priority (stretch goal) |
| `specs/deployment.md` | ✓ Complete | Hetzner deployment guide |

---

## Dependencies to Add

```toml
[dependencies]
# Terminal UI
ratatui = { optional = true, version = "0.29" }
crossterm = { optional = true, version = "0.28" }

# Commonware (verified available on crates.io)
commonware-cryptography = { optional = true, version = "0.0.64" }
commonware-consensus = { optional = true, version = "0.0.64" }
commonware-p2p = { optional = true, version = "0.0.64" }
commonware-runtime = { optional = true, version = "0.0.64" }
```

## Feature Flags to Add

```toml
[features]
tui = ["ratatui", "crossterm", "server"]
commonware = [
    "commonware-cryptography",
    "commonware-consensus",
    "commonware-p2p",
    "commonware-runtime",
    "server"
]
multiplayer = ["tui", "commonware"]
```

---

## Implementation Order

```
┌──────────────────────────────────────────────────────────────┐
│  Phase 1: Terminal UI                                        │
│  - Independent of other work                                 │
│  - Immediate user value                                      │
│  - ~2-3 days                                                 │
└──────────────────────────────────────────────────────────────┘
          │
          ▼
┌──────────────────────────────────────────────────────────────┐
│  Phase 2: Commonware RNG                                     │
│  - Core security requirement                                 │
│  - Modifies Deck::draw() signature                           │
│  - ~2-3 days                                                 │
└──────────────────────────────────────────────────────────────┘
          │
          ▼
┌──────────────────────────────────────────────────────────────┐
│  Phase 3: Consensus Layer                                    │
│  - Depends on RNG integration                                │
│  - Modifies Room coordination                                │
│  - ~3-4 days                                                 │
└──────────────────────────────────────────────────────────────┘
          │
          ▼
┌──────────────────────────────────────────────────────────────┐
│  Phase 4: Lobby & Polish                                     │
│  - Variable player count                                     │
│  - Room configuration                                        │
│  - ~1-2 days                                                 │
└──────────────────────────────────────────────────────────────┘
```

## Testing Strategy

| Component | Test Type | Approach |
|-----------|-----------|----------|
| TUI | Unit | Mock terminal backend (`ratatui::backend::TestBackend`) |
| RNG | Unit | Verify determinism: same seed → same shuffle |
| RNG | Integration | Commit-reveal with 2-4 in-process participants |
| Consensus | Unit | Commonware mock utilities |
| Consensus | Integration | Multi-process WebSocket test |
| E2E | System | Full game with TUI + consensus (manual + automated) |
