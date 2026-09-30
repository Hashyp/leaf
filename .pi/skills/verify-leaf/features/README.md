# Leaf verification map

Primary surface: interactive Rust TUI. Secondary surfaces: CLI flags and `--inline` stdout rendering. The fixture is `TESTING.md`; copy it to a per-run temporary directory. Start each recipe from a fresh instance unless stated otherwise. The baseline command is `cargo build`, followed by the disposable Herdr pane launch in [../SKILL.md](../SKILL.md). Run inside Herdr (`HERDR_ENV=1`). Keep proof outside the checkout; use a new pane and isolated `XDG_CONFIG_HOME`/`HOME` per run. Before driving, run the Doctor check. Capture the user's action and resulting pane, not just a final screenshot. For each entry point actually tried, record its outcome; a different entry point is not a substitute.

- [Render a Markdown file](render.md): file, stdin, and inline rendering.
- [Navigate and search](search.md): scroll, TOC, search and match traversal.
- [Open Markdown files](picker.md): fuzzy and classic pickers.
- [Watch changes](watch.md): reload a changed file and manual reload.
