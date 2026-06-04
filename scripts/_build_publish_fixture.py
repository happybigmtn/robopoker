#!/usr/bin/env python3
"""One-off generator: build the STW-032 publish-fixture.

The script drops a synthetic green receipt at a tempdir,
runs `trainer --publish <receipt>` with a known
`RBP_TRAINER_GIT_SHA`, then moves the resulting
`publish/<basename>/` to the committed fixture path
`crates/autotrain/tests/fixtures/publish-fixture/`.

The fixture is committed so a regression in the verifier
that would silently accept a malformed bundle fails the
new `publish::tests::run_verifies_committed_publish_fixture`
lib test on every `cargo test --workspace` run.
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

WORKSPACE = Path("/srv/dev/repos/robopoker")
TRAINER = WORKSPACE / "target" / "debug" / "trainer"
DEST = WORKSPACE / "crates" / "autotrain" / "tests" / "fixtures" / "publish-fixture"

if not TRAINER.exists():
    sys.stderr.write(f"trainer binary not found at {TRAINER}; run `cargo build -p trainer` first\n")
    sys.exit(1)

# Use a fixed basename so the bundle's tarball entry paths
# are byte-stable (the receipt basename is part of every
# tarball entry's path).
BASENAME = "testnet-live-proof-fixture"
RECEIPT_BIN = "/srv/dev/repos/robopoker/target/debug/trainer"
RECEIPT_DBURL = "<redacted: 49 chars>"

with tempfile.TemporaryDirectory() as tmp:
    tmp = Path(tmp)
    receipt_dir = tmp / "parent" / BASENAME
    publish_dir = tmp / "parent" / "publish" / BASENAME
    receipt_dir.parent.mkdir(parents=True, exist_ok=True)
    receipt_dir.mkdir(parents=True, exist_ok=True)

    # Use the lib's LiveProofReceipt::write_to to drop a
    # synthetic green receipt (the same shape the live
    # runbook produces). Drive it via a tiny Rust helper
    # binary.
    helper = WORKSPACE / ".cargo-target-tmp" / "write-fixture-receipt"
    if not helper.exists():
        # Build a one-shot helper via rustc against the
        # already-built autotrain rlib. The rlib is in
        # `target/debug/deps/`.
        sys.stderr.write("helper missing; falling back to manual receipt drop\n")

    # Manual receipt drop: write the 7 step dirs
    # (stdout/stderr/exit) + recipe.json + ENV.txt +
    # README.md so the fixture mirrors the on-disk
    # shape the real runbook produces. The SUMMARY.txt
    # is written *after* the runbook-shape stubs
    # (below) with a fixed `receipt_dir:` line so the
    # receipt bytes are byte-stable across
    # regenerations.
    (receipt_dir / "ENV.txt").write_text(
        "DATABASE_URL=<redacted>\n"
        "RBP_FAST_EPOCHS=2\n"
        "RBP_FAST_BATCH=16\n"
        "RBP_BENCH_HANDS=4\n"
        "RBP_BENCH_BLIND=2\n"
        "RBP_COMPARE_HANDS=4\n"
        "RBP_COMPARE_BLIND=2\n"
    )
    (receipt_dir / "README.md").write_text(
        "# STW-032 publish-fixture\n"
        "\n"
        "A committed no-DB portable reference bundle for the\n"
        "STW-032 publish step. The fixture is a synthetic\n"
        "green receipt + the deterministic\n"
        "`trainer --publish <receipt>` bundle a CI auditor\n"
        "can re-verify without a live Postgres.\n"
        "\n"
        "Do NOT edit the bundle files by hand. To regenerate:\n"
        "\n"
        "    rm -rf crates/autotrain/tests/fixtures/publish-fixture\n"
        "    python3 scripts/_build_publish_fixture.py\n"
        "\n"
        "The lib test\n"
        "`publish::tests::run_verifies_committed_publish_fixture`\n"
        "re-verifies the bundle on every `cargo test\n"
        "--workspace` run.\n"
    )
    for step in ("cluster", "reset", "smoke", "status", "bench", "compare", "replay"):
        sd = receipt_dir / step
        sd.mkdir()
        (sd / "stdout.txt").write_text("")
        (sd / "stderr.txt").write_text("")
        (sd / "exit.txt").write_text("0\n")
    # bench/transcripts/ subdir (empty) to mirror the
    # real runbook shape.
    (receipt_dir / "bench" / "transcripts").mkdir()
    (receipt_dir / "recipe.json").write_text(
        "{\n"
        f'  "trainer_bin": "{RECEIPT_BIN}",\n'
        f'  "database_url": "{RECEIPT_DBURL}",\n'
        '  "steps": [\n'
        '    { "name": "cluster",  "exit": 0, "stdout_bytes": 0, "stderr_bytes": 0 },\n'
        '    { "name": "reset",    "exit": 0, "stdout_bytes": 0, "stderr_bytes": 0 },\n'
        '    { "name": "smoke",    "exit": 0, "stdout_bytes": 0, "stderr_bytes": 0 },\n'
        '    { "name": "status",   "exit": 0, "stdout_bytes": 0, "stderr_bytes": 0 },\n'
        '    { "name": "bench",    "exit": 0, "stdout_bytes": 0, "stderr_bytes": 0 },\n'
        '    { "name": "compare",  "exit": 0, "stdout_bytes": 0, "stderr_bytes": 0 },\n'
        '    { "name": "replay",   "exit": 0, "stdout_bytes": 0, "stderr_bytes": 0 }\n'
        '  ]\n'
        "}\n"
    )

    # Normalise the receipt's SUMMARY.txt to a fixed
    # `receipt_dir:` line + a fixed `trainer:` line so
    # the receipt bytes are byte-stable across
    # regenerations. The real runbook writes the
    # actual receipt path; the fixture (a portable
    # reference a CI auditor re-verifies) uses
    # placeholders the real receipt's `runbook_version`
    # + receipt's basename already pin.
    (receipt_dir / "SUMMARY.txt").write_text(
        "testnet live_proof complete: smoke=12 status=12 bench=4 compare=4 replay=256\n"
        "\n"
        "  receipt_dir: <fixed-fixture-path>\n"
        f"  trainer:     {RECEIPT_BIN}\n"
    )

    # Drive the trainer binary with a fixed git SHA so
    # the manifest is byte-stable.
    env = os.environ.copy()
    env["RBP_TRAINER_GIT_SHA"] = "0123456789abcdef0123456789abcdef01234567"
    res = subprocess.run(
        [str(TRAINER), "--publish", str(receipt_dir)],
        capture_output=True, text=True, env=env
    )
    if res.returncode != 0:
        sys.stderr.write(
            f"trainer --publish failed (exit {res.returncode}):\n"
            f"  stdout: {res.stdout!r}\n"
            f"  stderr: {res.stderr!r}\n"
        )
        sys.exit(1)
    print(res.stdout.strip())

    if not publish_dir.is_dir():
        sys.stderr.write(f"publish dir missing: {publish_dir}\n")
        sys.exit(1)

    # Replace the committed fixture.
    if DEST.exists():
        shutil.rmtree(DEST)
    shutil.copytree(publish_dir, DEST)

    # Normalise the manifest's `receipt_dir` field to a
    # stable placeholder so the committed fixture is
    # byte-stable across regenerations (the runbook's
    # `receipt_dir` is always a tempdir path that
    # changes per-build). The lib verifier does NOT
    # gate on `receipt_dir` (a third party
    # re-verifying the bundle has no need to know
    # the original path), so normalising it is
    # safe.
    manifest_path = DEST / "manifest.json"
    with open(manifest_path) as f:
        manifest = json.load(f)
    manifest["receipt_dir"] = "/srv/dev/repos/robopoker/crates/autotrain/tests/fixtures/publish-fixture-source"
    with open(manifest_path, "w") as f:
        json.dump(manifest, f, indent=2)
        f.write("\n")
    # The manifest was rewritten, so the bundle
    # itself is still valid (the manifest is a
    # sibling of the tarball, not inside it). The
    # bundle.sha256 still matches the original
    # bundle.tar.gz (we did not modify the tarball).

    print(f"wrote fixture to {DEST}")
    for child in sorted(DEST.iterdir()):
        size = child.stat().st_size
        print(f"  {child.name} ({size} bytes)")
