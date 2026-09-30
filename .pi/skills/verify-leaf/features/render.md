# Render a Markdown file

Leaf presents headings, code, tables, lists, math, and styled inline text in the terminal.

## Sub-features

- `file`: `leaf TESTING.md` displays the manual fixture in the TUI.
- `stdin`: `cat TESTING.md | leaf` renders piped Markdown in the TUI.
- `inline`: `leaf --inline plain TESTING.md` renders plain text to stdout without opening the TUI.

## How to get to it (user POV)

Open `TESTING.md` using the file argument, pipe it to `leaf`, or use `--inline plain` for output to another command.

## Driving it with Herdr

Preconditions: `cargo build`; isolated config and a copied `TESTING.md`; Doctor checks the process for TUI paths.

- **File:** run the copy with `herdr pane run <id> 'env HOME=<scratch>/home XDG_CONFIG_HOME=<scratch>/config target/debug/leaf <copy>'` in a pane created by this run, capture the `Testing` heading, `Quick Start`, and status line; scroll with `d` toward the `Manual Fixture` section for tables and Unicode.
- **Stdin:** run `cat <copy> | target/debug/leaf` in a fresh Herdr pane and capture the same headings; expect a `stdin` label instead of a filename.
- **Inline:** run `target/debug/leaf --inline plain <copy>` with stdout and stderr redirected to proof files; record `$?`; check stdout for `tokyo-signal` and absence of ANSI escape sequences (`\x1b`).
- **Proof:** keep the commands and captures together; compare file and stdin content, not their status labels.

## Gotchas

- Stdin mode has no backing file, so watch/reload cannot work there.
- Herdr `pane read --source visible` captures reflect a viewport, not the whole document; scroll to inspect offscreen features.
- `--inline` is a secondary CLI output path, not a replacement for TUI proof.
