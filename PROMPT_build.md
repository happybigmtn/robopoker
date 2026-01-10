0a. Study `specs/*` to learn the application specifications for the terminal-based multiplayer poker environment.
0b. Study @IMPLEMENTATION_PLAN.md.
0c. Study `src/` to understand the existing robopoker codebase - cards, gameplay, mccfr, hosting, gameroom modules are already implemented.
0d. The existing codebase has: card representation (src/cards/), game mechanics (src/gameplay/), WebSocket hosting (src/hosting/), game room coordination (src/gameroom/), player abstractions (src/players/).

1. Your task is to implement functionality per the specifications. Follow @IMPLEMENTATION_PLAN.md and choose the most important item to address. Before making changes, search the codebase (don't assume not implemented) - robopoker already has extensive poker primitives. For commonware library integration (distributed RNG, consensus), use the commonware-library MCP server for current API docs.
2. After implementing functionality or resolving problems, run the tests for that unit of code that was improved. If functionality is missing then it's your job to add it as per the application specifications.
3. When you discover issues, immediately update @IMPLEMENTATION_PLAN.md with your findings. When resolved, update and remove the item.
4. When the tests pass, update @IMPLEMENTATION_PLAN.md, then `git add -A` then `git commit` with a message describing the changes. Include `Co-Authored-By: Warp <agent@warp.dev>` in commit messages. After the commit, `git push`.

99999. Important: When authoring documentation, capture the why — tests and implementation importance.
999999. Important: Single sources of truth, no migrations/adapters. If tests unrelated to your work fail, resolve them as part of the increment.
9999999. As soon as there are no build or test errors create a git tag. If there are no git tags start at 0.0.0 and increment patch by 1.
99999999. You may add extra logging if required to debug issues.
999999999. Keep @IMPLEMENTATION_PLAN.md current with learnings — future work depends on this to avoid duplicating efforts.
9999999999. When you learn something new about how to run the application, update @AGENTS.md but keep it brief.
99999999999. For any bugs you notice, resolve them or document them in @IMPLEMENTATION_PLAN.md.
999999999999. Implement functionality completely. Placeholders and stubs waste efforts and time redoing the same work.
9999999999999. When @IMPLEMENTATION_PLAN.md becomes large periodically clean out completed items.
99999999999999. If you find inconsistencies in the specs/* then update the specs with corrections.
999999999999999. IMPORTANT: Keep @AGENTS.md operational only — status updates belong in `IMPLEMENTATION_PLAN.md`.
9999999999999999. IMPORTANT: Reuse existing robopoker modules (cards, gameplay, gameroom, hosting) rather than reimplementing. Only add what's missing for multiplayer terminal play with commonware consensus.
