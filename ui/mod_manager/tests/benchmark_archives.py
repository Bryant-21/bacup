"""Install a supplied synthetic fixture into a disposable owned library."""
import argparse
from pathlib import Path
import tempfile

from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.package import Package


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--archive", type=Path, required=True)
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="manager-archive-benchmark-") as temporary:
        library = Library(Path(temporary) / "library", create=True)
        profile = library.create_profile("Main")
        package = Package(args.archive)
        try:
            plan = package.plain_plan()
            library.install(package, plan, "Synthetic benchmark", profile)
            assert len(library.rows(profile)) == 1
            print(f"{len(plan)} files, {sum(package.sizes.values()):,} source bytes installed; no game data")
        finally:
            package.close()


if __name__ == "__main__":
    main()
