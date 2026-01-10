# Multiplayer Networking Enhancements Specification

## Overview
Enhancements to existing robopoker networking for peer-to-peer multiplayer with consensus.

## Current State (robopoker)
- `src/hosting/` - Actix-web HTTP/WebSocket server
- `src/gameroom/` - Async room coordination, player actors
- Client-server architecture (server is authority)
- Single server holds game state

## Required Changes

### Peer-to-Peer Support
Current robopoker uses client-server. For fair multiplayer with commonware consensus:
- Each player runs a node
- No single authority for game state
- Consensus required for state transitions

### Options
1. **Hybrid**: Keep WebSocket for real-time, add commonware consensus layer
2. **Full P2P**: Replace server with peer mesh using commonware

### Recommended: Hybrid Approach
- Use existing WebSocket infrastructure for low-latency messaging
- Add commonware consensus for critical operations:
  - Deck shuffling (RNG)
  - Action ordering
  - Pot distribution
- Server becomes coordinator, not authority

### Game Lobby Enhancements
Extend existing hosting:
- Room creation with consensus parameters
- Join via room code or peer discovery
- Set game parameters (blinds, buy-in, max players)

### Message Protocol Extensions
Add to existing WebSocket messages:
- `consensus_request` / `consensus_response`
- `entropy_contribution` (for RNG)
- `state_hash` (for verification)

### Integration with gameroom
- Modify `src/gameroom/room.rs` to require consensus before state changes
- Add consensus verification to event broadcasting

## Acceptance Criteria
- [ ] Multiple players can join and play
- [ ] No single point of trust for game state
- [ ] Actions require consensus before applying
- [ ] Existing WebSocket infrastructure reused
- [ ] Graceful handling of disconnects
