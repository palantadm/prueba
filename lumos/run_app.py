"""
Lumos native desktop app launcher.
Run from the lumos/ directory:
    python run_app.py
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))

from app.main import main

if __name__ == "__main__":
    main()
