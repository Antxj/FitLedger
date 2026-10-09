"""Documentation screenshots with FAKE data (never your real activities).

Runs the app in screenshot mode (FITLEDGER_DEMO + FITLEDGER_SCREENSHOTS) in a
temporary folder, in Portuguese and English, and saves compressed PNGs to
docs/pt-BR and docs/en. Requires: pip install pillow

    cargo build --release
    python tools/screenshots.py target/release/fitledger.exe
"""
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SHOTS = ["dashboard", "activity-charts", "activity-splits", "analysis", "calendar", "goals", "strength", "tennis", "insights"]


def main():
    exe = Path(sys.argv[1] if len(sys.argv) > 1 else ROOT / "target" / "release" / "fitledger.exe").resolve()
    for lang, folder in (("pt", "pt-BR"), ("en", "en")):
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp) / "home"
            shots = Path(tmp) / "shots"
            home.mkdir()
            env = dict(os.environ)
            env.update(
                FITLEDGER_HOME=str(home),
                FITLEDGER_DEMO="1",
                FITLEDGER_SCREENSHOTS=str(shots),
                FITLEDGER_SCREENSHOT_ONLY=",".join(SHOTS),
                FITLEDGER_LANGUAGE=lang,
                FITLEDGER_INSTANCE="docs",
            )
            subprocess.run([str(exe)], env=env, check=True, timeout=600)
            out = ROOT / "docs" / folder
            out.mkdir(parents=True, exist_ok=True)
            for name in SHOTS:
                src = shots / f"{name}.png"
                if not src.exists():
                    print("missing", src)
                    continue
                Image.open(src).convert("RGB").save(out / f"{name}.png", optimize=True)
            print(folder, "ok")


if __name__ == "__main__":
    main()
