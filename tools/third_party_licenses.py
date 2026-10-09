"""Writes THIRD-PARTY-LICENSES.txt with every crate compiled into the Windows
executable (normal dependencies only) and the license texts they ship.

Run from the repository root after changing dependencies:
    python tools/third_party_licenses.py
"""
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGET = "x86_64-pc-windows-gnu"
LICENSE_PREFIXES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE", "COPYRIGHT")


def main():
    meta = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", TARGET],
            cwd=ROOT,
        )
    )
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    root = meta["resolve"]["root"]

    # walk normal (non-dev, non-build) dependencies from the root
    seen, stack = set(), [root]
    while stack:
        pid = stack.pop()
        if pid in seen:
            continue
        seen.add(pid)
        for dep in nodes[pid]["deps"]:
            kinds = [k["kind"] for k in dep["dep_kinds"]]
            if any(k is None for k in kinds):
                stack.append(dep["pkg"])
    seen.discard(root)

    out = [
        "THIRD-PARTY COMPONENTS DISTRIBUTED WITH FITLEDGER",
        "=================================================",
        "",
        "FitLedger itself is free software, licensed under the GNU General Public",
        "License version 3 or later (GPL-3.0-or-later) - full text in the LICENSE",
        "file. Source code: https://github.com/Antxj/FitLedger",
        "",
        "FitLedger.exe is built from the Rust crates listed below. Their licenses",
        "follow, as shipped by each crate. SQLite (bundled by libsqlite3-sys) is in",
        "the public domain. The fonts bundled by epaint_default_fonts (Ubuntu, Hack,",
        "Noto Emoji, emoji-icon-font) carry their own licenses, included below.",
        "",
        "SUMMARY",
        "-------",
    ]
    pkgs = sorted((packages[i] for i in seen), key=lambda p: (p["name"], p["version"]))
    for p in pkgs:
        out.append(f"{p['name']} {p['version']}  -  {p.get('license') or 'see license file'}  -  {p.get('repository') or ''}")
    out += ["", "", "LICENSE TEXTS", "-------------"]
    texts_seen = {}
    for p in pkgs:
        crate_dir = Path(p["manifest_path"]).parent
        files = sorted(
            f for f in crate_dir.rglob("*")
            if f.is_file()
            and f.name.upper().startswith(LICENSE_PREFIXES)
            and "target" not in f.parts
            and f.stat().st_size < 200_000
        )
        out += ["", "=" * 78, f"{p['name']} {p['version']} ({p.get('license') or 'see below'})", "=" * 78]
        if not files:
            out.append("(no license file in the crate; license: " + (p.get("license") or "unknown") + ")")
        for f in files:
            text = f.read_text(encoding="utf-8", errors="replace").strip()
            key = hash(text)
            rel = f.relative_to(crate_dir)
            if key in texts_seen:
                out.append(f"--- {rel}: same text as {texts_seen[key]}")
                continue
            texts_seen[key] = f"{p['name']} {p['version']} / {rel}"
            out += [f"--- {rel}", text]
    (ROOT / "THIRD-PARTY-LICENSES.txt").write_text("\n".join(out) + "\n", encoding="utf-8")
    print(f"{len(pkgs)} crates")


if __name__ == "__main__":
    main()
