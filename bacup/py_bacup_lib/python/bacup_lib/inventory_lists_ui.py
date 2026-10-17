from __future__ import annotations

from pathlib import Path

from bacup_lib.inspect_ui import rebuild
from bacup_lib.legendary_perks_ui import swf_tags, verify_script_only
from bacup_lib.workshop_ui import uncompressed

RESOURCES = Path(__file__).with_name("resources") / "shared"
LIST = "Shared.AS3.BSScrollingList"


def patch_inventory_list(movie: bytes) -> bytes:
    from creation_lib.swf import native_runtime as native

    if LIST not in native.abc_class_names(movie):
        return movie
    if "B21GetSource" in {row["method"] for row in native.abc_disassemble(movie, LIST)}:
        raise ValueError("Inventory adapter already present; rebuild from the stock movie")
    helpers = [RESOURCES / f"{name}.as" for name in ("B21InventoryRules", "B21InventoryIcon", "B21InventoryControls")]
    helper = native.compile_as3_do_abc([path.read_text(encoding="utf-8") for path in helpers])
    tags = list(swf_tags(uncompressed(movie)))
    first_script = next(i for i, (code, _) in enumerate(tags) if code == 82)
    tags.insert(first_script, (82, helper))
    patched = native.augment_as3_classes(rebuild(uncompressed(movie), tags), {
        LIST: (RESOURCES / "B21InventoryList.as").read_text(encoding="utf-8")})
    if "InvListEntry" in native.abc_class_names(patched):
        patched = native.augment_as3_classes(patched, {
            "InvListEntry": (RESOURCES / "B21InventoryEntry.as").read_text(encoding="utf-8")})
    patched = native.patch_as3_method(patched, LIST, "get selectedIndex",
        [["getproperty", "iSelectedIndex"]], [["callproperty", "B21GetSelectedIndex", 0]])
    patched = native.patch_as3_method(patched, LIST, "set selectedIndex",
        [["callpropvoid", "doSetSelectedIndex", 1]], [["callpropvoid", "B21SelectEngineIndex", 1]])
    patched = native.patch_as3_method(patched, LIST, "get entryList",
        [["getproperty", "EntriesA"]], [["callproperty", "B21GetSource", 0]])
    patched = native.patch_as3_method(patched, LIST, "set entryList",
        [["getlocal", 0], ["getlocal", 1]],
        [["getlocal", 0], ["getlocal", 0], ["getlocal", 1], ["callproperty", "B21SetSource", 1]])
    patched = native.patch_as3_method(patched, LIST, "InvalidateData",
        [["getlocal", 0], ["getproperty", "_filterer"], ["getlocal", 0], ["getproperty", "EntriesA"], ["setproperty", "filterArray"]],
        [["getlocal", 0], ["callpropvoid", "B21Reproject", 0], *[["keep", i] for i in range(5)]])
    patched = native.patch_as3_method(patched, LIST, "InvalidateData",
        [["getlocal", 0], ["callpropvoid", "UpdateList", 0]],
        [["keep", 0], ["keep", 1], ["getlocal", 0], ["callpropvoid", "B21FinishProjection", 0]])
    for method in ("SetFocusUnderMouse", "moveSelectionUp", "moveSelectionDown"):
        bodies = native.abc_disassemble(patched, LIST, method)
        for operation, instruction in (("getproperty", "GetProperty selectedIndex"), ("setproperty", "SetProperty selectedIndex"),
                                       ("initproperty", "InitProperty selectedIndex")):
            count = sum(instruction in line for body in bodies for line in body["code"])
            if count:
                patched = native.patch_as3_method(patched, LIST, method,
                    [[operation, "selectedIndex"]], [[operation, "B21ViewIndex"]], expected_matches=count)
    if "Pipboy_InvPage" in native.abc_class_names(patched):
        patched = native.patch_as3_method(patched, "Pipboy_InvPage", "onPipboyChangeEvent",
            [["getproperty", "filterer"], ["getlocal", 1], ["getproperty", "DataObj"], ["getproperty", "InvSelectedItems"]],
            [["keep", 1], ["keep", 2], ["keep", 3]])
        patched = native.patch_as3_method(patched, "Pipboy_InvPage", "onPipboyChangeEvent",
            [["callproperty", "ClampIndex", 1]], [["callproperty", "B21ClampNativeIndex", 1]])
    verify_script_only(movie, patched)
    from bacup_lib.inventory_icon_art import inject_inventory_icons
    return inject_inventory_icons(patched)


def build_inventory_lists(stock_interface: Path, output_data: Path) -> list[str]:
    paths = []
    for name in ("pipboymenu.swf", "pipboy_invpage.swf"):
        destination = output_data / "Interface" / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(patch_inventory_list((stock_interface / name).read_bytes()))
        paths.append(str(destination))
    return paths


def convert_inventory_lists(target_data: Path, output_data: Path) -> list[str]:
    from creation_lib.ba2.native_runtime import extract_one

    archive = target_data / "Fallout4 - Interface.ba2"
    paths = []
    for name in ("pipboymenu.swf", "pipboy_invpage.swf"):
        stock = extract_one(str(archive), f"interface/{name}")
        if not stock:
            raise FileNotFoundError(f"Missing stock {name} in {archive}")
        destination = output_data / "Interface" / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(patch_inventory_list(stock))
        paths.append(str(destination))
    return paths


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("stock_interface", type=Path)
    parser.add_argument("output_data", type=Path)
    args = parser.parse_args()
    for path in build_inventory_lists(args.stock_interface, args.output_data):
        print(path)
