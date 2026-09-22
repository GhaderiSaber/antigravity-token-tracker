#!/usr/bin/env python3
import sys
import os

# Add package directory to python path
current_dir = os.path.dirname(os.path.abspath(__file__))
if current_dir not in sys.path:
    sys.path.insert(0, current_dir)

from antigravity_tracker.cli import main

if __name__ == "__main__":
    sys.exit(main() or 0)
