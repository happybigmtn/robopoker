# Commonware Integration Specification

## Overview
Integration with the commonware library for verifiable randomness and distributed consensus in multiplayer poker. This REPLACES the current `rand` crate usage in robopoker.

## Current State (robopoker)
- Uses `rand` crate with `small_rng` feature for deck shuffling
- Single-node random generation (not verifiable)
- No distributed consensus

## Required Changes

### Distributed Random Number Generation
- Replace `rand::rngs::SmallRng` with commonware's distributed RNG
- Ensure randomness is verifiable by all participants
- No single party can manipulate or predict the deck order
- Commit-reveal scheme for fair dealing

### Where RNG is Used (files to modify)
- `src/cards/deck.rs` - Deck shuffling
- Any test files using random generation

### Consensus Requirements
- All players must agree on game state before proceeding
- Actions are ordered and finalized via consensus
- Handle network partitions gracefully
- Byzantine fault tolerance for cheating prevention

### Card Dealing Protocol
1. Each participant contributes entropy
2. Combine entropy via commonware consensus
3. Derive deterministic shuffle from combined seed
4. Deal cards in agreed order
5. Reveal cards at appropriate game stages

### State Synchronization
- Game state replicated across all nodes
- State transitions validated by consensus
- Integrate with existing `gameroom` event system

## MCP Integration
- Use commonware MCP server (https://mcp.commonware.xyz) for API reference
- Query for current library version and patterns
- Follow recommended Rust integration patterns

## Acceptance Criteria
- [ ] RNG seed derived from multiple participant contributions
- [ ] Shuffle is deterministic given the seed
- [ ] All participants can verify shuffle correctness
- [ ] Game state transitions require consensus
- [ ] Network failures handled without corruption
- [ ] No single participant can cheat the RNG
- [ ] Existing robopoker tests still pass
