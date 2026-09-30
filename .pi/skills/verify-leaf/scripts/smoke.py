#!/usr/bin/env python3
"""Drive Leaf in a disposable Herdr pane and preserve visible TUI proof."""
import argparse
import json
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[4]
BINARY = (ROOT / "target/debug/leaf").resolve()


def herdr(*args, json_result=False):
    result = subprocess.run(("herdr", *args), text=True, capture_output=True, check=True)
    return json.loads(result.stdout) if json_result else result.stdout


def capture(pane, proof, name, needle):
    # Wait for a *visible* state, not a match from old scrollback.
    found = herdr("pane", "wait-output", pane, "--match", needle,
                  "--source", "visible", "--timeout", "10000", json_result=True)
    screen = found["result"]["read"]["text"]
    if needle not in screen:
        raise RuntimeError(f"Expected {needle!r} in visible pane")
    (proof / name).write_text(screen)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("proof", type=Path, help="New persistent proof directory outside the checkout")
    args = parser.parse_args()
    proof = args.proof.expanduser().resolve()
    if proof == ROOT or ROOT in proof.parents:
        parser.error("Proof must be outside the checkout")
    if proof.exists():
        parser.error(f"Proof path already exists: {proof}")
    if not BINARY.is_file():
        parser.error("Build Leaf first: cargo build")
    # Herdr requires the caller to be inside a managed pane; never target UI focus.
    import os
    if os.environ.get("HERDR_ENV") != "1" or not os.environ.get("HERDR_PANE_ID"):
        parser.error("Run inside a Herdr-managed pane (HERDR_ENV=1)")
    current = os.environ["HERDR_PANE_ID"]
    layout = herdr("pane", "layout", "--pane", current, json_result=True)["result"]["layout"]
    rect = next(p["rect"] for p in layout["panes"] if p["pane_id"] == current)
    direction = "right" if rect["width"] >= 140 else "down"
    if (direction == "right" and rect["width"] < 120) or (direction == "down" and (rect["width"] < 65 or rect["height"] < 40)):
        parser.error("Not enough room for a verification pane; enlarge the Herdr tab")

    proof.mkdir(parents=True)
    pane = None
    with tempfile.TemporaryDirectory(prefix="leaf-verify-") as tmp:
        scratch = Path(tmp)
        (scratch / "home").mkdir()
        (scratch / "config").mkdir()
        fixture = scratch / "TESTING.md"
        shutil.copyfile(ROOT / "TESTING.md", fixture)
        try:
            created = herdr("pane", "split", "--current", "--direction", direction,
                            "--cwd", str(scratch), "--no-focus", json_result=True)
            pane = created["result"]["pane"]["pane_id"]
            command = (f"env HOME={shlex.quote(str(scratch / 'home'))} "
                       f"XDG_CONFIG_HOME={shlex.quote(str(scratch / 'config'))} "
                       f"{shlex.quote(str(BINARY))} {shlex.quote(str(fixture))}")
            herdr("pane", "run", pane, command)
            capture(pane, proof, "01-initial.txt", "Quick Start")
            # Doctor: inspect only the exact pane created by this run.
            info = herdr("pane", "process-info", "--pane", pane, json_result=True)["result"]["process_info"]
            processes = info["foreground_processes"]
            if len(processes) != 1 or Path(f"/proc/{processes[0]['pid']}/exe").resolve() != BINARY or processes[0]["argv"][:2] != [str(BINARY), str(fixture)]:
                raise RuntimeError(f"Wrong foreground process in {pane}: {processes}")
            (proof / "02-doctor.txt").write_text(f"pane={pane} pid={processes[0]['pid']} exe={BINARY}\nfixture={fixture}\n")
            herdr("pane", "send-keys", pane, "/")
            capture(pane, proof, "03-search-open.txt", "/  enter confirm · esc cancel")
            herdr("pane", "send-text", pane, "tokyo-signal")
            capture(pane, proof, "04-search-draft.txt", "/tokyo-signal  enter confirm")
            herdr("pane", "send-keys", pane, "enter")
            capture(pane, proof, "05-search-result.txt", "1/7  n/N next/prev")
            herdr("pane", "send-keys", pane, "n")
            capture(pane, proof, "06-next-match.txt", "2/7  n/N next/prev")
            herdr("pane", "send-keys", pane, "q")
        finally:
            if pane is not None:
                # Close only our pane, never another pane or the Herdr server.
                herdr("pane", "close", pane)
    if not (proof / "06-next-match.txt").is_file():
        raise RuntimeError("Proof was lost during cleanup")
    print(f"PASS: Herdr TUI search/next-match proof in {proof}")


if __name__ == "__main__":
    main()
