# Navigate and search

Find repeated text in a rendered Markdown file and navigate the document without leaving the TUI.

## Sub-features

- `search-confirm`: `/` opens search, typing a query and pressing Enter shows a match count.
- `search-next`: `n` and `N` traverse repeated matches.
- `scroll`: `j`/`k`, `d`/`u`, and `g`/`G` move through the document.
- `toc`: `t` toggles the table of contents; `1`–`9` jump to visible TOC entries.

## How to get to it (user POV)

Run `leaf TESTING.md`, then press the keys above while viewing the document. `/` starts search.

## Driving it with Herdr

Preconditions: build `target/debug/leaf`; launch a disposable Herdr pane on a copy of `TESTING.md` as in [../SKILL.md](../SKILL.md); Doctor reports the expected binary.

- **Open:** send `herdr pane send-keys <id> /`; capture a pane showing `/  enter confirm · esc cancel`.
- **Type:** send `herdr pane send-text <id> tokyo-signal`; capture `/tokyo-signal  enter confirm`.
- **Confirm:** send `herdr pane send-keys <id> enter`; wait for `1/7  n/N next/prev` in the status line.
- **Next:** send `herdr pane send-keys <id> n`; wait for `2/7  n/N next/prev` and capture the pane showing `tokyo-signal` in the document.
- **Proof:** run `.pi/skills/verify-leaf/scripts/smoke.py /tmp/leaf-verification-<unique-id>` after `cargo build`; it captures these states as `01`–`06` and checks the doctor. For scroll/TOC, send `g`, `G`, `t`, or a TOC digit and compare before/after content and the status line.

## Gotchas

- `tokyo-signal` is below the initial viewport, so the initial capture should show `Quick Start`, not the match.
- Typing a term is not confirmation: require a `1/7` counter after Enter and `2/7` after `n`.
- `Esc` while editing cancels the draft; after confirmation it clears the active search.
