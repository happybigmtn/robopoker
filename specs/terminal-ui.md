# Terminal UI Specification

## Overview
Text-based user interface for poker gameplay in the terminal. This EXTENDS the existing robopoker `players` module.

## Current State (robopoker)
- `src/players/human.rs` - Basic human player with `dialoguer` for input
- `src/hosting/` - WebSocket-based game hosting
- No dedicated terminal UI rendering

## Required Changes

### New TUI Module
Create `src/tui/` for terminal interface:
- Render game state visually
- Handle keyboard input
- Display cards, chips, pot, players
- Show available actions

### Display Layout
```
╔══════════════════════════════════════════════════════════════╗
║  Pot: $150                              Blinds: $1/$2        ║
╠══════════════════════════════════════════════════════════════╣
║  Player 2: $98        Player 3: $150       Player 4: $75     ║
║  [Dealer]             [Small Blind]        [Big Blind]       ║
║                                                              ║
║                    ┌─────────────────┐                       ║
║                    │  [Ah] [Kd] [7s] │  Community Cards      ║
║                    │  [2c] [  ]      │                       ║
║                    └─────────────────┘                       ║
║                                                              ║
║  ► You: $120                                                 ║
║    Hand: [As] [Ac]                                           ║
╠══════════════════════════════════════════════════════════════╣
║  Actions: [F]old  [C]heck  [R]aise  Amount: ___              ║
╚══════════════════════════════════════════════════════════════╝
```

### Integration Points
- Implement `Player` trait from `src/players/mod.rs`
- Subscribe to game events from `src/gameroom/`
- Use existing `Card` display implementations from `src/cards/`

### Dependencies
- `ratatui` for terminal rendering
- `crossterm` for input handling
- Already using `tokio` for async

### Features
- Clear display of community cards
- Player positions and chip counts
- Current pot size
- Dealer button and blind positions
- Your hole cards (hidden from others)
- Available actions with keyboard shortcuts
- Action history/log

## Acceptance Criteria
- [ ] TUI renders game state correctly
- [ ] Keyboard shortcuts work for all actions
- [ ] Actions validated before sending
- [ ] Works in 80x24 minimum terminal size
- [ ] Integrates with existing Player trait
- [ ] Handles game events from gameroom
