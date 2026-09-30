# Watch changes

Leaf reloads a file after it changes on disk and shows the updated rendering without restarting.

## Sub-features

- `auto`: `leaf --watch TESTING.md` watches and reloads a changed file.
- `manual`: press `r` while watching to force a reload.
- `toggle`: press `w` to toggle watch while viewing a file.

## How to get to it (user POV)

Run `leaf --watch TESTING.md` or press `w` while viewing `TESTING.md`; edit the file from another terminal and observe the result in Leaf.

## Driving it with Herdr

Preconditions: `cargo build`; a private copy of `TESTING.md` in scratch; launch in a new Herdr pane with `--watch` on that copy, never the repository original. Doctor verifies the process.

- **Baseline:** capture the pane and the copied fixture before editing.
- **Edit:** replace `watch-reload-marker` in the copied file with a unique token; capture the edit command and subsequent file contents as independent evidence of the disk change.
- **Observe:** wait for the `⟳ reloaded` status indicator, search for the unique token with `/`, `Enter`, and capture its match count and rendered location.
- **Manual:** alter the copy again, send `r`, capture the reload feedback and new text. Press `w` and observe the status when testing toggle.
- **Proof:** preserve both file snapshots and pane captures. Only remove the copied fixture during scratch cleanup.

## Gotchas

- Reload requires a file-backed input; stdin cannot be watched.
- Reload status is transient; poll and capture promptly rather than sleeping indefinitely.
- A successful write to disk alone does not prove the TUI reloaded; search for the changed token inside the app.
