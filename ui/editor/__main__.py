"""NIF editor runs only as part of the toolkit.

Launch via: uv run python -m ui.toolkit
"""
import sys

print("The standalone NIF editor has been removed.", file=sys.stderr)
print("Launch the toolkit instead: uv run python -m ui.toolkit", file=sys.stderr)
sys.exit(1)
