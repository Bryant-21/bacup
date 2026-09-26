from __future__ import annotations

import argparse
from pathlib import Path

from export_stamps import build_catalog, query, read_stock, write_catalog


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--target", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    roots = {"TadpoleBadges": 0x3FC7DF, "PossumBadges": 0x426912}
    stocks = read_stock(args.source, list(roots.values()))
    ids = {int(row["form_id"], 16) for items in stocks.values() for row in items}
    filters = [value for key in sorted(ids) for value in ("--record", f"{key:06X}")]
    targets = query(args.target, "fo4", "form_id,eid,signature", *filters)
    catalogs = {name: build_catalog(stocks[root], targets, args.target.name) for name, root in roots.items()}
    for name, catalog in catalogs.items():
        write_catalog(args.output_dir / f"{name}.json", catalog)


if __name__ == "__main__":
    main()
