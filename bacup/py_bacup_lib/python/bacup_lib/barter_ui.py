from __future__ import annotations

import hashlib
import json
from pathlib import Path

from bacup_lib.inspect_ui import rebuild
from bacup_lib.legendary_perks_ui import imports, remap_menu_fonts, swf_tags, verify_script_only
from bacup_lib.ui_contract import resolve_numbered_as3_call
from bacup_lib.workshop_ui import art_library, uncompressed

SOURCE = "securetrade.swf"
MENU = Path("Interface/BarterMenu.swf")
CONTAINER_MENU = Path("Interface/ContainerMenu.swf")
OUTPUT = Path("Interface/B21/TalesFromAppalachia/Barter")
BRIDGE = Path(__file__).with_name("resources") / "barter/BarterMenu.as"
CONTAINER_LIST_SETTER_STEM = "__setProp_ContainerList_mc_MenuObj_ContainerList_"


def convert_barter_ui(source_root: Path, output_data: Path, *, target_data_dir: Path) -> dict:
    from creation_lib.ba2.native_runtime import extract_one

    archive = Path(target_data_dir) / "Fallout4 - Interface.ba2"
    stock = extract_one(str(archive), "interface/bartermenu.swf")
    container = extract_one(str(archive), "interface/containermenu.swf")
    if not stock or not container:
        raise FileNotFoundError(f"Missing stock barter or container menu in {archive}")
    return build_barter_ui(source_root, output_data, stock, container)


def _patch_trade_menu(native, stock: bytes, bridge: str, document: str, sort_hint: tuple[list, list]) -> tuple[bytes, str]:
    movie = native.augment_as3_classes(stock, {"ContainerMenu": bridge})
    movie = native.patch_as3_method(movie, document, "UpdateButtonHints", *sort_hint)
    container_list_setter = resolve_numbered_as3_call(
        native,
        stock,
        document,
        "$constructor",
        CONTAINER_LIST_SETTER_STEM,
        f"{document} container-list setter",
    )
    movie = native.patch_as3_method(movie, document, "$constructor",
        [["getlocal", 0], ["callpropvoid", container_list_setter, 0]],
        [["keep", 0], ["keep", 1], ["getlocal", 0], ["callpropvoid", "B21Initialize", 0]])
    movie = native.patch_as3_method(movie, "ContainerMenu", "InvalidateLists",
        [["callpropvoid", "ValidateListHighlight", 0]],
        [["callpropvoid", "B21ValidateListHighlight", 0]])
    movie = native.patch_as3_method(movie, "ContainerMenu", "RepositionUpperBracketBars",
        [["getlocal", 0], ["getproperty", "ContainerInventory_mc"], ["getlocal", 1], ["callpropvoid", "addChild", 1]],
        [["keep", 0], ["keep", 1], ["keep", 2], ["keep", 3], ["getlocal", 0], ["callpropvoid", "B21HideStockChrome", 0]])
    return movie, container_list_setter


def build_barter_ui(source_root: Path, output_data: Path, stock: bytes, container_stock: bytes | None = None) -> dict:
    from creation_lib.swf import native_runtime as native

    interface = Path(source_root) / "interface"
    source = (interface / SOURCE).read_bytes()
    library, names = art_library(source, {
        "SecureTradePlayerInventory": "B21TFA_TradePlayerPanel",
        "SecureTradeOfferInventory": "B21TFA_TradeOfferPanel",
        "Shared.AS3.LabelSelector": "B21TFA_TradeTabs",
    })
    required = ("SecureTradePlayerInventory", "SecureTradeOfferInventory", "Shared.AS3.LabelSelector")
    if not set(required) <= names.keys():
        raise ValueError("FO76 trade panel/category artwork is missing")
    bridge = BRIDGE.read_text(encoding="utf-8")
    for index, name in enumerate(required):
        bridge = bridge.replace(f"/* ART_{index} */", names[name])
    # Barter reads SortButton through its own subclass; the container movie's document class is ContainerMenu.
    movie, container_list_setter = _patch_trade_menu(native, stock, bridge, "BarterMenu", (
        [["getlex", "SortButton"]],
        [["getlocal", 0], ["keep", 0], ["callproperty", "B21SortHint", 1]]))
    movie = native.patch_as3_method(movie, "BarterMenu", "UpdateButtonHints",
        [["getlocal", 0], ["getproperty", "InvestButton"], ["getlocal", 2]],
        [["getlocal", 0], ["keep", 0], ["keep", 1], ["callproperty", "B21InvestHint", 1], ["keep", 2]])
    menus = [(MENU, movie, stock)]
    if container_stock is not None:
        container, _ = _patch_trade_menu(native, container_stock, bridge, "ContainerMenu", (
            [["getlocal", 0], ["getproperty", "SortButton"]],
            [["getlocal", 0], ["keep", 0], ["keep", 1], ["callproperty", "B21SortHint", 1]]))
        menus.append((CONTAINER_MENU, container, container_stock))
    for _, patched, original in menus:
        verify_script_only(original, patched)
    files = {path.name.casefold(): path for path in interface.iterdir() if path.is_file()}
    pending, dependencies = imports(library), {}
    while pending:
        name = pending.pop().casefold()
        if name in dependencies:
            continue
        if "/" in name or "\\" in name or not name.endswith(".swf"):
            raise ValueError(f"Unsupported barter import: {name}")
        dependencies[name] = files[name].read_bytes()
        pending.extend(imports(dependencies[name]))

    def private_imports(data: bytes) -> bytes:
        tags = []
        for code, payload in swf_tags(remap_menu_fonts(data)):
            if code in (57, 71):
                name, tail = payload.split(b"\0", 1)
                payload = name.lower() + b"\0" + tail
            tags.append((code, payload))
        return rebuild(uncompressed(data), tags)

    outputs = menus + [(OUTPUT / SOURCE, private_imports(library), source)]
    outputs += [(OUTPUT / name, private_imports(data), data) for name, data in sorted(dependencies.items())]
    receipt = {"schema_version": 1, "source": SOURCE, "menu": MENU.as_posix(),
               "target_container_list_setter": container_list_setter,
               "bridge_sha256": hashlib.sha256(bridge.encode()).hexdigest(), "files": []}
    for relative, data, original in outputs:
        path = Path(output_data) / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        receipt["files"].append({"path": relative.as_posix(), "sha256": hashlib.sha256(data).hexdigest(),
                                 "source_sha256": hashlib.sha256(original).hexdigest()})
    (Path(output_data) / OUTPUT / "conversion.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    return receipt


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser()
    parser.add_argument("source_root", type=Path)
    parser.add_argument("target_movie", type=Path)
    parser.add_argument("output_data", type=Path)
    parser.add_argument("--container-movie", type=Path)
    args = parser.parse_args()
    container = args.container_movie.read_bytes() if args.container_movie else None
    print(json.dumps(build_barter_ui(args.source_root, args.output_data, args.target_movie.read_bytes(), container), indent=2))
