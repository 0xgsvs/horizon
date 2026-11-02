#!/usr/bin/env python3
"""Union-merge the launcher's translation tables without duplicate JSON keys.

The language files all carry the same keys, so two branches that add translations append at the
same end of every file and conflict. This merges the JSON objects by key instead: keys from the
right side are added when the left side lacks them, a key the right side deleted is deleted when
the left side left it alone, and a key both sides changed differently is refused rather than
picked silently.

Usage:
  python3 tools/merge_lang_json.py                  # resolve conflicted launcher/lang/*.json in jj/git
  python3 tools/merge_lang_json.py BASE OURS THEIRS  # Git merge driver, writes OURS
  python3 tools/merge_lang_json.py BASE LEFT RIGHT OUT # Jujutsu merge tool, writes OUT

For Jujutsu, add this to the user config and run `jj resolve --tool json-union`:

  [merge-tools.json-union]
  program = "python3"
  merge-args = ["tools/merge_lang_json.py", "$base", "$left", "$right", "$output"]
  merge-conflict-exit-codes = [1]
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parent.parent
LANG = ROOT / "launcher" / "lang"


def merge(base: dict[str, Any], left: dict[str, Any], right: dict[str, Any]) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for key, value in left.items():
        if key in right:
            if value == right[key]:
                out[key] = value
                continue
            if key not in base:
                raise ValueError(f"both sides added {key!r} with different values")
            if base[key] == value:
                out[key] = right[key]
            elif base[key] == right[key]:
                out[key] = value
            else:
                raise ValueError(f"both sides changed {key!r} differently")
        elif key not in base:
            out[key] = value
        elif base[key] != value:
            raise ValueError(f"both sides changed {key!r}, one of them deleted it")
    for key, value in right.items():
        if key not in left and key not in base:
            out[key] = value
    return out


def read_object(path: Path) -> dict[str, Any]:
    text = path.read_text(encoding="utf-8")
    value = json.loads(text)
    if not isinstance(value, dict) or not all(isinstance(k, str) for k in value):
        raise ValueError(f"{path}: expected an object of strings")
    return value


def write_object(path: Path, value: dict[str, Any]) -> None:
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def merge_files(base: Path, left: Path, right: Path, out: Path) -> None:
    try:
        value = merge(read_object(base), read_object(left), read_object(right))
    except (OSError, ValueError, json.JSONDecodeError) as e:
        print(f"merge_lang_json: {e}", file=sys.stderr)
        raise SystemExit(1)
    write_object(out, value)


def git_show(stage: int, path: str) -> bytes:
    return subprocess.run(
        ["git", "show", f":{stage}:{path}"],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
    ).stdout


def jj_paths(output: str) -> list[str]:
    return [
        line.split()[0]
        for line in output.splitlines()
        if line.startswith("launcher/lang/") and "conflict" in line
    ]


def conflicted_paths() -> list[str]:
    if (ROOT / ".jj").is_dir():
        proc = subprocess.run(
            ["jj", "resolve", "--list"],
            cwd=ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        return jj_paths(proc.stdout)
    proc = subprocess.run(
        [
            "git",
            "diff",
            "--name-only",
            "--diff-filter=U",
            "--",
            "launcher/lang/*.json",
        ],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
        text=True,
    )
    return [line for line in proc.stdout.splitlines() if line]


def resolve_conflicts() -> int:
    paths = conflicted_paths()
    if not paths:
        print("merge_lang_json: no conflicted language files")
        return 0
    for path in paths:
        # Stage 1 is the merge base, 2 the left/ours side, 3 the right/theirs side.
        try:
            base = json.loads(git_show(1, path))
        except subprocess.CalledProcessError:
            base = {}
        left = json.loads(git_show(2, path))
        right = json.loads(git_show(3, path))
        try:
            value = merge(base, left, right)
        except ValueError as e:
            print(f"merge_lang_json: {path}: {e}", file=sys.stderr)
            return 1
        write_object(ROOT / path, value)
        print(f"merged {path}")
    return 0


def main(argv: list[str]) -> int:
    if len(argv) == 1:
        return resolve_conflicts()
    if len(argv) == 4:
        merge_files(Path(argv[1]), Path(argv[2]), Path(argv[3]), Path(argv[2]))
        return 0
    if len(argv) == 5:
        merge_files(Path(argv[1]), Path(argv[2]), Path(argv[3]), Path(argv[4]))
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
