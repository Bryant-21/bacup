from __future__ import annotations

import json
import re
import struct

from bacup_lib.quest_area_ui import closure
from bacup_lib.status_hud_source import sprite_tags, states, transform
from bacup_lib.ui_contract import resolve_numbered_name
from creation_lib.swf.native_runtime import abc_disassemble
from creation_lib.swf.tags import DefineShapeTag


def bitmap_dependencies(tags, selected):
    wanted = set()
    for code, payload in selected:
        if code not in (2, 22, 32, 83):
            continue
        shape = DefineShapeTag.parse(payload, code).shape
        fills = list(shape.fill_styles)
        for record in shape.records:
            fills.extend(getattr(record, "new_fill_styles", None) or [])
        wanted.update(fill.bitmap_id for fill in fills
                      if fill.fill_type in (0x40, 0x41, 0x42, 0x43) and fill.bitmap_id != 0xffff)
    found = {struct.unpack_from("<H", p)[0]: (c, p) for c, p in tags if c in (6, 20, 21, 35, 36, 90)}
    if missing := wanted - found.keys():
        raise ValueError(f"Missing HUD bitmap dependencies: {sorted(missing)}")
    return {found[cid] for cid in wanted}


def reward_symbols(symbols):
    return {resolve_numbered_name(symbols, "HUDMenu_fla." + stem + "_", "HUD reward"): name
            for stem, name in (("uniqueItemContainer_mc", "B21_StatusRewardItem"),
                               ("questRewardContainer_mc", "B21_StatusRewardList"),
                               ("questCompleteContainer_mc", "B21_StatusRewardQuest"))}


def frame_actions(method):
    stack, actions = [], []
    for line in method["code"]:
        instruction = line.strip().split("  ", 1)[1]
        if instruction.startswith(("Debug", "PushScope", "ReturnVoid")):
            if instruction == "PushScope":
                stack.pop()
            continue
        if instruction == "GetLocal { index: 0 }":
            stack.append("this")
        elif instruction.startswith("FindPropStrict "):
            stack.append("this")
        elif instruction.startswith("GetProperty "):
            stack.append(stack.pop() + "." + instruction.split(" ", 1)[1])
        elif instruction.startswith("PushString "):
            stack.append(json.loads(instruction.split(" ", 1)[1]))
        elif instruction in ("PushTrue", "PushFalse"):
            stack.append(instruction == "PushTrue")
        elif instruction.startswith(("PushByte ", "PushShort ")):
            stack.append(int(re.search(r"value: (\d+)", instruction)[1]))
        elif instruction.startswith("ConstructProp flash.events.Event "):
            count = int(re.search(r"\((\d+)\)", instruction)[1])
            values = stack[-count:]
            del stack[-count:]
            stack.pop()
            stack.append(values[0])
        elif instruction.startswith("CallPropVoid "):
            call, count = re.fullmatch(r"CallPropVoid (.*) \((\d+)\)", instruction).groups()
            count = int(count)
            values = stack[-count:] if count else []
            if count:
                del stack[-count:]
            target = stack.pop().removeprefix("this").lstrip(".")
            if call == "stop":
                actions.append(["stop", target])
            elif call in ("gotoAndPlay", "gotoAndStop"):
                actions.append(["goto", target, values[0], call == "gotoAndPlay"])
            elif call == "dispatchEvent":
                actions.append(["event", values[0]])
            elif not call.startswith("HUDMenu_fla.__setTab_"):
                raise ValueError(f"Unsupported reward frame action: {instruction}")
        else:
            raise ValueError(f"Unsupported reward frame instruction: {instruction}")
    return actions


def reward_contract(source, tags, symbols, wanted):
    sprites = {struct.unpack_from("<H", p)[0]: p for c, p in tags if c == 39}
    ids = {struct.unpack_from("<H", p)[0] for name in wanted
           for c, p in closure(tags, symbols[name], name) if c == 39}
    timelines = {}
    loader = {}
    names = {cid: name for name, cid in symbols.items()}
    for cid in sorted(ids):
        labels, frame = {}, 1
        for code, payload in sprite_tags(sprites[cid]):
            if code == 43:
                labels[payload.split(b"\0")[0].decode()] = frame
            elif code == 1:
                frame += 1
        actions = {}
        if cid in names:
            for method in abc_disassemble(source, names[cid]):
                match = re.fullmatch(r"HUDMenu_fla.frame(\d+)", method["method"])
                if match:
                    actions[match[1]] = frame_actions(method)
                if "__setProp_ClipContainer_mc_questAnimCatcher_mc" in method["method"]:
                    for index, line in enumerate(method["code"]):
                        field = line.strip().split("SetProperty ")[-1]
                        if field in ("maxClipHeight", "questAnimStageHeight", "questAnimStageWidth"):
                            loader[field] = int(re.search(r"value: (\d+)", method["code"][index-1])[1])
        timelines[str(cid)] = {"labels": labels, "actions": actions}
    root = states(tags)[0]["AnnounceEventWidget_mc"]
    children = states(sprite_tags(sprites[symbols["HUDAnnounceEventWidget"]]))[0]
    layout = {}
    for key, child in (("item", "UniqueItemContainer_mc"), ("list", "QuestRewardContainer_mc"),
                       ("quest", "QuestCompleteContainer_mc")):
        a, b, c, d, x, y = transform(root)
        e, f, g, h, u, v = transform(children[child])
        layout[key] = [a*e+c*f, b*e+d*f, a*g+c*h, b*g+d*h, a*u+c*v+x, b*u+d*v+y]
    if set(loader) != {"maxClipHeight", "questAnimStageHeight", "questAnimStageWidth"}:
        raise ValueError("Missing source reward quest-image sizing")
    return ids, {"timelines": timelines, "layout": layout, "fps": 30, "loader": loader}
