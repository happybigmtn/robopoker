# BP Bridge Closeout - 2026-06-10

## Mandate

Build the smallest bridge from robopoker's latest trained blueprint to the Arena Starter Kit's existing blueprint seam, without modifying the starter kit floor path, touching Vast.ai, or registering a new dev.fun agent.

## Chosen Bridge Shape

Implemented a local Postgres-to-JSON exporter:

```text
trainer --export-blueprint <out.json> [--blueprint v1|v2|v3]
```

The default export target is v3 (`BLUEPRINT3`), matching the seed's latest-blueprint requirement. This shape was chosen over an HTTP server because the starter kit already accepts a local JSON `--blueprint-path`, and over a symlink because robopoker's source of truth is Postgres rows, not a ready file artifact.

## Contract

The exporter writes the starter kit's live `l1_v1` JSON shape:

```json
{
  "header": {
    "source": "robopoker-postgres",
    "coarse": true,
    "schema": "l1_v1",
    "entries": 1
  },
  "entries": [
    {
      "key": {
        "street": "Preflop",
        "position": "BTN",
        "hand_class": "AJo",
        "facing_action": "open"
      },
      "action": "raise",
      "amount_chips": 60
    }
  ]
}
```

The starter kit was inspected read-only at `/srv/dev/repos/poker-arena/poker-arena-starter-kit`. No starter-kit files were modified, including the L1 floor path. The exporter adds metadata only under `header`, which the loader ignores for action lookup.

## Implementation Notes

- Added `crates/autotrain/src/export_blueprint.rs` and wired it through `crates/autotrain/src/mode.rs`.
- Reads `BLUEPRINT`, `BLUEPRINT2`, or `BLUEPRINT3`, with v3 as the default.
- Projects preflop abstract rows into concrete L1 hand classes using `ISOMORPHISM`.
- Groups rows by `(past, present, position)` and exports the max cumulative-weight edge as the deterministic starter-kit action.
- Emits starter-kit action names: `fold`, `check`, `call`, `raise`.
- Uses `RBP_EXPORT_BLUEPRINT_BIG_BLIND_CHIPS` for chip sizing, defaulting to 20 chips.

## Scope Limits

This first bridge is intentionally preflop-only. Robopoker stores postflop decisions as abstract bucket IDs, while the starter kit seam currently keys lookups by concrete L1 hand class and facing-action strings. A postflop bridge should define an explicit board/texture key contract before exporting those rows.

`Edge::Shove` is skipped for now because a static JSON artifact cannot safely infer the current stack-size all-in amount for every Arena state.

## Verification

- `AUTO_SKIP_REMOTE_SYNC=1 auto doctor` passed. `auto doctor --yolo` is not accepted by this repo's `auto 0.2.0` CLI.
- `rustfmt --edition 2024 --check --config skip_children=true crates/autotrain/src/export_blueprint.rs crates/autotrain/src/mode.rs crates/autotrain/src/lib.rs` passed.
- `git diff --check` on the edited bridge files passed.
- `AUTO_SKIP_REMOTE_SYNC=1 cargo check -p rbp-autotrain` passed.
- `AUTO_SKIP_REMOTE_SYNC=1 cargo test -p rbp-autotrain --lib` passed.
- A Python smoke test loaded a representative exported `l1_v1` sample through the starter kit's `examples/strategy.py::load_blueprint`.

`cargo fmt --all --check` was not used as a gate because the workspace already has unrelated rustfmt drift outside this bridge slice.

## Guardrails Observed

- Did not touch Vast.ai.
- Did not register a new dev.fun agent.
- Did not modify `/srv/dev/repos/poker-arena/poker-arena-starter-kit`.
- Left pre-existing dirty clustering files unmodified and unstaged.
- Used `AUTO_SKIP_REMOTE_SYNC=1` for local verification.

## Sources Read

- `/srv/dev/repos/poker-arena/poker-arena-starter-kit/examples/agent.py`
- `/srv/dev/repos/poker-arena/poker-arena-starter-kit/examples/strategy.py`
- `/srv/dev/repos/poker-arena/poker-arena-starter-kit/tests/test_strategy.py`
- `https://arena.dev.fun/skills/arena.md`
- `https://arena.dev.fun/skills/texas-holdem.md`
- `https://arena.dev.fun/skills/poker-eval.md`

## Next Slice

Run `trainer --export-blueprint out.json` against the latest trained Postgres receipt DB, then pass that file to the starter kit with `--blueprint-path` and verify Arena dry-run behavior before expanding the bridge beyond preflop.
