# Open Markdown files

Choose a file to view from the fuzzy picker or classic directory browser.

## Sub-features

- `fuzzy`: launch without a file from a terminal to open the fuzzy Markdown picker.
- `browser`: launch with `--picker` to browse directories.
- `switch`: from a file view, `Ctrl+P` opens the fuzzy picker and `P` opens the browser.

## How to get to it (user POV)

Run `leaf` or `leaf --picker` from a directory with Markdown files, or press `Ctrl+P` / `Shift+P` while viewing one.

## Driving it with Herdr

Preconditions: `cargo build`; a private scratch working directory containing two distinct `.md` files, e.g. `alpha.md` and `beta.md`; Doctor verifies the process.

- **Fuzzy:** launch `target/debug/leaf` from the scratch directory using `herdr pane run <id> '<checkout>/target/debug/leaf'` (absolute binary path) (PTY stdin is required). Type `alpha` literally, capture the filtered picker, press `Enter`, and capture the `alpha.md` filename and its unique contents.
- **Browser:** launch `<checkout>/target/debug/leaf --picker` from scratch in a fresh Herdr pane, use `j`/`k` or arrow keys to select `beta.md`, capture the selected row, press `Enter`, then capture its contents.
- **Switch:** while viewing `alpha.md`, send `C-p` or `P`, select `beta.md`, then verify the title and rendered text change. Keep before/action/after pane captures for each path attempted.

## Gotchas

- A pipe into Leaf changes its input mode; launch the picker in a Herdr pane with PTY stdin, not piped stdin.
- The fuzzy picker treats plain `j` and `k` as query text; use `Down`/`Up` or `Ctrl+J`/`Ctrl+K` to move its selection.
- Verify selected content as well as filename; multiple files can have similar names.
