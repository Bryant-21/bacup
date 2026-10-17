"""Absolute-path bootstrap for the opt-in Windows protocol registration."""
import sys
from pathlib import Path


def main():
    if len(sys.argv) != 2:
        raise SystemExit("Expected one Nexus browser link.")
    sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
    from creation_lib.mod_manager.nexus import NexusError, parse_nxm
    try:
        parse_nxm(sys.argv[1])
    except NexusError as exc:
        raise SystemExit(str(exc)) from None
    from ui.toolkit.__main__ import run_toolkit_variant
    run_toolkit_variant("mod_manager", launch_path=sys.argv[1])


if __name__ == "__main__":
    main()
