# Terminal Multiplayer Poker with Commonware Integration

## Overview
Build a terminal-based multiplayer poker environment where players can play No-Limit Texas Hold'em over a network with verifiable randomness and distributed consensus.

## Current State Analysis

### Existing Functionality (Complete)
- **Cards** (`src/cards/`): Card representation, deck, hand evaluation (bitwise), equity calculation, hole cards, board
- **Gameplay** (`src/gameplay/`): Complete game engine with betting rules, actions, showdown logic, settlements, payouts
- **Gameroom** (`src/gameroom/`): Async room coordination, `Player` trait, `Actor` wrapper, `Event` broadcasting, `Channel`, `Recall` state
- **Hosting** (`src/hosting/`): WebSocket server via Actix-web, `Casino` room management, `Client` network player, room bridging
- **Players** (`src/players/`): `Human` player with `dialoguer` CLI input; `Fish` CPU in `src/gameroom/players/`

### What Needs Implementation
1. **Terminal UI** - Visual game display (ratatui/crossterm)
2. **Commonware RNG** - Replace `rand::random_range()` with distributed verifiable randomness
3. **Consensus Layer** - Game state agreement across all participants

---

## Priority 1: Terminal UI Module (specs/terminal-ui.md)
**Status**: Not started  
**Existing Code**: `src/players/human.rs` uses `dialoguer` for basic CLI prompts (no visual rendering)

### Tasks
- [ ] Create `src/tui/mod.rs` module structure
- [ ] Add dependencies: `ratatui`, `crossterm` to Cargo.toml
- [ ] Implement `TuiRenderer` for game state visualization:
  - Board/community cards display
  - Player positions with chip counts
  - Pot size, blinds, dealer button
  - Hole cards (private to player)
  - Action history/log panel
- [ ] Implement `TuiPlayer` implementing `Player` trait from `src/gameroom/player.rs`
- [ ] Add keyboard input handling (F=Fold, C=Check/Call, R=Raise, number input for amounts)
- [ ] Support 80x24 minimum terminal size
- [ ] Integrate with existing `Event` system from `src/gameroom/event.rs`

### Integration Points
- Implement `Player` trait: `decide(&mut self, recall: &Recall) -> Action`
- Subscribe to `Event::Play`, `Event::ShowHand`, `Event::NextHand`, `Event::YourTurn`
- Use existing `Card` display implementations from `src/cards/card.rs`

---

## Priority 2: Commonware RNG Integration (specs/commonware-integration.md)
**Status**: Not started  
**Existing Code**: `src/cards/deck.rs:32` uses `rand::random_range(0..n)` for card selection

### Commonware Primitives Available
- `commonware-cryptography`: BLS12-381 DKG, threshold signatures
- `commonware-consensus`: Byzantine consensus (simplex)
- `commonware-p2p`: Authenticated peer communication
- `commonware-collector`: Collect responses for committable requests

### Tasks
- [ ] Add commonware dependencies to Cargo.toml (behind feature flag `commonware`)
- [ ] Create `src/rng/mod.rs` module for distributed randomness
- [ ] Implement `DistributedRng` trait abstracting RNG source:
  - Local mode: Use existing `rand` for single-player/testing
  - Distributed mode: Entropy contribution from all participants
- [ ] Implement commit-reveal protocol for fair shuffling:
  - Each participant generates entropy commitment
  - Reveal phase after all commits received
  - Combine revealed entropy deterministically (XOR or hash)
  - Derive shuffle seed from combined entropy
- [ ] Modify `Deck::draw()` to accept RNG source parameter
- [ ] Modify `Game::default()` and `Game::next()` to use distributed RNG
- [ ] Ensure deterministic shuffle given same seed across all nodes

### Verification Requirements
- All participants can verify shuffle correctness
- No single party can manipulate deck order
- Existing tests must pass with local RNG mode

---

## Priority 3: Multiplayer Consensus Layer (specs/multiplayer-enhancements.md)
**Status**: Not started  
**Existing Code**: `src/hosting/` is client-server (server is authority), `src/gameroom/room.rs` manages single source of truth

### Recommended Architecture: Hybrid Approach
Keep existing WebSocket infrastructure for real-time messaging, add commonware consensus for critical operations.

### Tasks
- [ ] Create `src/consensus/mod.rs` module
- [ ] Define consensus-required operations:
  - Deck shuffling (RNG seed agreement)
  - Action ordering and validation
  - Pot distribution at showdown
- [ ] Extend WebSocket message protocol (`src/hosting/client.rs`):
  - `entropy_contribution` messages
  - `consensus_request` / `consensus_response`
  - `state_hash` for verification
- [ ] Modify `Room::run()` (`src/gameroom/room.rs`) to require consensus before state changes
- [ ] Implement state hash verification:
  - Compute hash of `Game` state after each action
  - Broadcast hash to all participants
  - Detect and handle state divergence
- [ ] Handle network partitions gracefully (timeout + fallback)
- [ ] Add Byzantine fault tolerance for cheating prevention

### Integration with Existing Gameroom
- `Room` becomes coordinator, not authority
- Actions validated by consensus before `apply()`
- Event broadcast includes state hash

---

## Priority 4: Game Lobby Enhancements
**Status**: Partially exists  
**Existing Code**: `src/hosting/server.rs` has `/start`, `/enter/{room_id}`, `/leave/{room_id}` endpoints

### Tasks
- [ ] Add room configuration options (blinds, buy-in, max players)
- [ ] Implement room listing endpoint
- [ ] Add room discovery/join by code
- [ ] Configure consensus parameters per room
- [ ] Support variable player counts (currently hardcoded `N=2` in `src/lib.rs:36`)

---

## Existing TODOs in Codebase
Found via grep search:
- `src/gameplay/game.rs:321` - Edge case for all-in blinds
- `src/cards/observation.rs:45` - TODO marker
- `src/gameplay/recall.rs:69` - TODO marker
- `src/cards/street.rs:265,281` - TODO markers
- `src/gameplay/abstraction.rs:294,310` - TODO markers
- `src/mccfr/nlhe/encoder.rs:172,175` - TODO markers

These are implementation details within the MCCFR/abstraction system, not blocking for multiplayer poker.

---

## Specifications
All three required specifications exist:
- `specs/terminal-ui.md` ✓
- `specs/commonware-integration.md` ✓
- `specs/multiplayer-enhancements.md` ✓

---

## Dependencies to Add
```toml
# Terminal UI
ratatui = { optional = true, version = "0.28" }
crossterm = { optional = true, version = "0.28" }

# Commonware (version TBD based on crates.io)
commonware-cryptography = { optional = true, version = "0.0.64" }
commonware-consensus = { optional = true, version = "0.0.64" }
commonware-p2p = { optional = true, version = "0.0.64" }
commonware-runtime = { optional = true, version = "0.0.64" }
```

## Feature Flags to Add
```toml
[features]
tui = ["ratatui", "crossterm", "server"]
commonware = ["commonware-cryptography", "commonware-consensus", "commonware-p2p", "commonware-runtime", "server"]
multiplayer = ["tui", "commonware"]
```

---

## Implementation Order
1. **Terminal UI** (can be developed independently, immediate user value)
2. **Commonware RNG** (core security requirement, affects `Deck`)
3. **Consensus Layer** (depends on RNG integration)
4. **Lobby Enhancements** (polish, can be incremental)

---

## Testing Strategy
- Unit tests for TUI rendering (mock terminal)
- Integration tests for RNG with local mode
- Consensus tests using commonware test utilities
- End-to-end multiplayer tests with multiple processes
