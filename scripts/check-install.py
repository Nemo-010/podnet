#!/usr/bin/env python3
"""Check the installer core of podcrates in scripts/install/.

  python3 -I scripts/check-install.py [--write] [--self-test]

The rules:

* Each file of scripts/install/SOURCES (core.sh, core-net.sh, pins.py) has the SHA-256
  of its line. These are the files of PodUtils/podcrates at the tag of the
  file, with no change.
* scripts/install.sh holds core.sh and core-net.sh inline, in this order,
  between the lines of the block of the core, as `pins.py stamp` puts them. The copy of the tree must run
  alone (`sh -c "$s"`), so it cannot include core.sh by a path.
* The block of pins of scripts/install.sh is empty: only a release writes
  pins (`pins.py stamp` in release.yml).

--write puts the core inline in scripts/install.sh (after a take of a new
version). --self-test runs the check on planted faults: a changed byte of
core.sh, a changed byte of the inline core, and a block with pins. It exits
0 only when the check finds each fault and passes the control copy.

Exit 0: no fault. Exit 1: a fault, one line for each.
"""

import hashlib
import shutil
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BEGIN = "# BEGIN PINS (scripts/install/pins.py writes this block; do not edit it)"
END = "# END PINS"
CORE_BEGIN = "# BEGIN CORE (scripts/install/pins.py puts scripts/install/core.sh here)"
# The files of the core, in the order of their include (pins.py, CORE).
CORE = ("core.sh", "core-net.sh")
CORE_END = "# END CORE"


def inline(d: Path) -> str:
    """The block of the core, as inline_core of pins.py writes it."""
    bodies = []
    for name in CORE:
        text = (d / name).read_text(encoding="utf-8")
        bodies.append((text.split("\n", 1)[1] if text.startswith("#!") else text).rstrip())
    return f"{CORE_BEGIN}\n" + "\n".join(bodies) + f"\n{CORE_END}\n"


def block(text: str, begin: str, end: str) -> tuple[int, int] | None:
    start, stop = text.find(begin + "\n"), text.find(end + "\n")
    if start < 0 or stop < start:
        return None
    return start, stop + len(end) + 1


def check(root: Path) -> list[str]:
    faults = []
    d = root / "scripts/install"
    for line in (d / "SOURCES").read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if len(parts) != 2 or len(parts[0]) != 64:
            continue
        f = d / parts[1]
        if not f.is_file():
            faults.append(f"{f}: missing (scripts/install/SOURCES names it)")
            continue
        have = hashlib.sha256(f.read_bytes()).hexdigest()
        if have != parts[0]:
            faults.append(f"{f}: the SHA-256 is {have}, not {parts[0]} of SOURCES")
    script = root / "scripts/install.sh"
    text = script.read_text(encoding="utf-8")
    span = block(text, CORE_BEGIN, CORE_END)
    if span is None:
        faults.append(f"{script}: no block of the core")
    elif text[span[0] : span[1]] != inline(d):
        faults.append(f"{script}: the inline core is not scripts/install/{' and '.join(CORE)} (run --write)")
    span = block(text, BEGIN, END)
    want = f'{BEGIN}\nINST_VERSION=""\nINST_TARGETS=""\n{END}\n'
    if span is None:
        faults.append(f"{script}: no block of pins")
    elif text[span[0] : span[1]] != want:
        faults.append(f"{script}: the block of pins is not empty: only a release writes pins")
    return faults


def write(root: Path) -> None:
    script = root / "scripts/install.sh"
    text = script.read_text(encoding="utf-8")
    span = block(text, CORE_BEGIN, CORE_END)
    if span is None:
        raise SystemExit(f"check-install: {script}: no block of the core")
    core = inline(root / "scripts/install")
    script.write_text(text[: span[0]] + core + text[span[1] :], encoding="utf-8")
    print(f"check-install: {script}: the core is inline")


def self_test() -> int:
    def plant(tree: Path, path: str, old: str, new: str) -> None:
        f = tree / path
        text = f.read_text(encoding="utf-8")
        if old not in text:
            raise SystemExit(f"check-install: self-test: {old!r} is not in {path}")
        f.write_text(text.replace(old, new, 1), encoding="utf-8")

    plants = {
        "core": ("scripts/install/core.sh", "inst_die()", "inst_die ()"),
        "inline": ("scripts/install.sh", "--proto-redir", "--proto-redirect"),
        "pins": ("scripts/install.sh", 'INST_VERSION=""', 'INST_VERSION="1.0.0"'),
    }
    rc = 0
    with tempfile.TemporaryDirectory() as tmp:
        for name in ["control", *plants]:
            tree = Path(tmp) / name
            (tree / "scripts").mkdir(parents=True)
            shutil.copytree(ROOT / "scripts/install", tree / "scripts/install")
            shutil.copy(ROOT / "scripts/install.sh", tree / "scripts/install.sh")
            if name in plants:
                plant(tree, *plants[name])
            found = check(tree)
            if (name == "control") == bool(found):
                print(f"check-install: self-test: {name}: wrong result {found}")
                rc = 1
            else:
                print(f"check-install: self-test: {name}: ok")
    return rc


def main(argv: list[str]) -> int:
    if "--self-test" in argv:
        return self_test()
    if "--write" in argv:
        write(ROOT)
    faults = check(ROOT)
    for f in faults:
        print(f"check-install: {f}")
    if not faults:
        print("check-install: the core is the pinned file of podcrates, inline, with no pins")
    return 1 if faults else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
