---
name: verify-leaf
description: Verify Leaf's Rust terminal Markdown viewer through its real TUI in a disposable Herdr pane. Use when changing rendering, search, navigation, file picking, or watch behavior.
---

# Verify Leaf (TUI)

Read [features/README.md](features/README.md) and the relevant feature file. Use the real `target/debug/leaf` process, not `ratatui::TestBackend` alone. Requires Linux (`/proc`), Python 3, Cargo, and **a Herdr-managed caller pane** (`HERDR_ENV=1`). Load the locally installed [Herdr skill](../herdr/SKILL.md) before controlling panes. From outside Herdr, stop and ask to run inside it; never control another user's focused session.

## Launch

From the checkout root, run `cargo build` once. A TUI has no service to keep alive: each drive creates a sibling Herdr pane with `herdr pane split --current --direction down|right --cwd <scratch> --no-focus`. The pane runs the binary on a private copy of `TESTING.md`, with private `HOME` and `XDG_CONFIG_HOME`. The smoke helper chooses a split that leaves enough width/height; enlarge the tab if it refuses:

```bash
.pi/skills/verify-leaf/scripts/smoke.py "/tmp/leaf-verification-$(date +%s%N)-$$"
```

Pass a **new** absolute proof path outside the checkout. The helper launches, drives, and closes only the pane it created. For other features, use its Herdr-pane pattern with a fresh scratch directory and fixture; parse the returned `.result.pane.pane_id` rather than guessing an ID.

## Doctor

Before sending keys, call `herdr pane process-info --pane <created-pane-id>` and verify its foreground process is this checkout's `target/debug/leaf` with the expected fixture argument (`/proc/<pid>/exe` resolves to the built binary). Require `Quick Start` in `herdr pane read <id> --source visible`. If the process or pane is wrong, stop the run. `smoke.py` performs these read-only checks.

## Drive

Use `herdr pane send-keys <id> /`, `herdr pane send-text <id> tokyo-signal`, and `herdr pane send-keys <id> enter` for search. Wait on visible output with `herdr pane wait-output <id> --match <text> --source visible --timeout 10000`, then save `herdr pane read <id> --source visible` (or the wait response's `.result.read.text`). Never route input through the UI-focused pane. The helper is the executable search-and-next-match recipe; other entry points and keys are in the feature map and `TESTING.md`.

## Evidence

Proof lives in the passed `/tmp/leaf-verification-*` directory and survives teardown. `smoke.py` preserves initial, search-open, search-draft, confirmed-match, and next-match pane captures plus the doctor result; it requires the real `1/7` then `2/7` counters. For other features capture before/action/after panes, independent file/history side effects and CLI stdout/stderr/exit codes where relevant. Exercise user-visible paths, not internal setters. Mocks are appropriate only at existing production boundaries; independently check what any dry-run actually skips (files, network, git refs).

## Cleanup

Quit Leaf with `q` when appropriate, then `herdr pane close <created-pane-id>` in a `finally` block, targeting only the pane returned by this run's split. Remove its disposable scratch/config/history; retain proof. The helper cleans up after failure and verifies evidence after success. Never close the caller pane, another user's pane, or the Herdr server.

## Helpers

`python3 .pi/skills/verify-leaf/scripts/smoke.py /tmp/leaf-verification-<unique-id>` (or run the executable directly). Build Leaf first; run inside Herdr. The proof path must not already exist.
