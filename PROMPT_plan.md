0a. Study `specs/*` to learn the application specifications.
0b. Study @IMPLEMENTATION_PLAN.md (if present) to understand the plan so far.
0c. Study the existing robopoker codebase in `src/` - this is our foundation:
    - `src/cards/` - Card representation, hand evaluation, deck, equity calculation
    - `src/gameplay/` - Game engine, betting rules, showdown logic, payout
    - `src/gameroom/` - Async game coordination, player actors, event broadcasting
    - `src/hosting/` - WebSocket server, room management
    - `src/players/` - Player trait and implementations (human, compute)
    - `src/mccfr/` - Monte Carlo CFR solver (for AI opponents)

1. Study @IMPLEMENTATION_PLAN.md (if present; it may be incorrect) and study existing source code in `src/*` and compare it against `specs/*`. Analyze findings, prioritize tasks, and create/update @IMPLEMENTATION_PLAN.md as a bullet point list sorted in priority of items yet to be implemented. Consider searching for TODO, minimal implementations, placeholders, skipped/flaky tests, and inconsistent patterns. Keep @IMPLEMENTATION_PLAN.md up to date with items considered complete/incomplete.

IMPORTANT: Plan only. Do NOT implement anything. Do NOT assume functionality is missing; confirm with code search first. The robopoker codebase already has extensive poker primitives - DO NOT reinvent them.

ULTIMATE GOAL: Build a terminal-based multiplayer poker environment where players can play No-Limit Texas Hold'em against each other over a network. Key requirements:
- Use the commonware library (via MCP at https://mcp.commonware.xyz) for verifiable random number generation and distributed consensus
- Terminal UI for game interaction (adapt or extend existing hosting/player code)
- Multiplayer networking for real-time gameplay (extend existing WebSocket hosting)
- Fair and cryptographically secure card dealing via commonware's distributed randomness (replace current RNG)
- Game state consensus across all participants (new feature using commonware)

The existing robopoker provides: cards, hand evaluation, game mechanics, showdown, WebSocket hosting, game rooms. What's needed: terminal UI, commonware RNG integration, distributed consensus for game state.

Consider missing elements and plan accordingly. If an element is missing, search first to confirm it doesn't exist, then if needed author the specification at specs/FILENAME.md. If you create a new element then document the plan to implement it in @IMPLEMENTATION_PLAN.md.
