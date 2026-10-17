"""Condition-list editor (the leveled-list condition builder as a widget over a list value)."""

from __future__ import annotations

import copy
from typing import Any

from imgui_bundle import imgui

from creation_lib.ui.widgets.modern import semantic_color
from ui.tools.catalog_editor.widgets import templates
from ui.tools.catalog_editor.widgets.functions import (
    ANSWER_ROLES, BY_ID, FALLOUT4, FO76, FUNCTIONS, Param, answered_by, label as function_label,
)

OPS = ("==", "!=", ">", ">=", "<", "<=")
RUN_ONS = ("subject", "target", "reference", "instance_owner", "active_players", "potential_players")
_RUN_ON_NAMES = ["Subject (the container/actor)", "Target (player for item lists)", "Reference",
                 "Instance owner", "Active players", "Potential players"]
_REFERENCE = ("REFR", "ACHR")

_state: dict[str, dict[str, Any]] = {}


def answer_label(answer: str) -> str:
    return f"answered by {answer}" if answer != "falls back" else "falls back to Fallout 4's roll"


def answer_mark(answer: str, right_align: bool = True) -> None:
    """Who answers a check in game, by default right-aligned in the current line."""
    text = answer_label(answer)
    if right_align:
        right = imgui.get_cursor_pos_x() + imgui.get_content_region_avail().x - imgui.calc_text_size(text).x
        imgui.set_cursor_pos_x(max(imgui.get_cursor_pos_x(), right))
    imgui.text_colored(semantic_color(ANSWER_ROLES[answer]), text)
    if answer == "falls back":
        imgui.set_item_tooltip("Tales can't answer this check, so any leveled list using it is rolled by Fallout 4 instead.")


def new_condition(function: int = 77) -> dict:
    return {"function": function, "op": "==", "value": 1.0, "run_on": "subject", "params": [None, None]}


class _Editor:
    """One frame of editing over a private copy of the conditions list."""

    def __init__(self, rows: Any, ctx, path: str) -> None:
        self.rows: list = copy.deepcopy(rows) if isinstance(rows, list) else []
        self.ctx = ctx
        self.path = path
        self.dirty = False
        self.state = _state.setdefault(path, {"editing": None, "filter": ""})

    # -- helpers --

    def describe(self, key: str) -> str:
        if not key:
            return "none"
        return (self.ctx.forms.describe(key) if self.ctx.forms else None) or key

    def name(self, key: str) -> str:
        return self.describe(key).split(" [", 1)[0]

    def changed(self, flag: bool) -> bool:
        self.dirty = self.dirty or flag
        return flag

    def ask(self, slot: str, signatures, current: str = "") -> None:
        self.ctx.picker.request(f"{self.path}#{slot}", signatures, current)

    def apply_landed(self) -> None:
        prefix = f"{self.path}#"
        for slot in [s for s in self.ctx.picker.landed if s.startswith(prefix)]:
            picked = self.ctx.picker.take(slot)
            kind, *args = slot[len(prefix):].split(":")
            index = int(args[0])
            if index >= len(self.rows):
                continue
            condition = self.rows[index]
            if kind == "form":
                templates.set_form(condition, picked)
            elif kind == "key":
                if picked:
                    condition[args[1]] = picked
                else:
                    condition.pop(args[1], None)
            elif kind == "param":
                params = condition.setdefault("params", [None, None])
                params[int(args[1])] = {"source": picked, "form": picked} if picked else None
            self.dirty = True

    def key_button(self, index: int, owner: dict, key: str, title: str, signatures) -> None:
        current = owner.get(key, "")
        if imgui.small_button(f"{title}: {self.name(current) if current else 'none'}##{key}"):
            self.ask(f"key:{index}:{key}", signatures, current)
        if current:
            imgui.set_item_tooltip(self.describe(current))
            imgui.same_line()
            if imgui.small_button(f"x##clear{key}"):
                owner.pop(key, None)
                self.dirty = True

    # -- list of checks --

    def draw(self) -> None:
        self.apply_landed()
        rows, state = self.rows, self.state
        if state["editing"] is not None and state["editing"] >= len(rows):
            state["editing"] = None
        if not rows:
            imgui.text_disabled("No checks: always passes")
        for index in range(len(rows)):
            imgui.push_id(index)
            if index:
                imgui.text_disabled("or" if rows[index - 1].get("or") else "and")
                imgui.same_line()
            sentence = templates.sentence(rows[index], self.describe)
            answer = answered_by(rows[index].get("function"))
            width = imgui.get_content_region_avail().x - 40 - imgui.calc_text_size(answer_label(answer)).x
            if imgui.selectable(f"{sentence}##chip", state["editing"] == index, 0, imgui.ImVec2(width, 0))[0]:
                state["editing"] = index
            imgui.same_line()
            answer_mark(answer, right_align=False)
            imgui.same_line()
            if imgui.small_button("x"):
                rows.pop(index)
                state["editing"] = None
                self.dirty = True
                imgui.pop_id()
                break
            imgui.pop_id()
        if len(rows) > 1:
            mode = templates.chain_mode(rows)
            if imgui.radio_button("all checks pass", mode == "all") and mode != "all":
                templates.set_chain_mode(rows, "all")
                self.dirty = True
            imgui.same_line()
            if imgui.radio_button("any check passes", mode == "any") and mode != "any":
                templates.set_chain_mode(rows, "any")
                self.dirty = True
            if mode == "mixed":
                imgui.text_disabled("Mixed and/or (set per check under Advanced).")
        if state["editing"] is not None:
            imgui.separator()
            self.draw_check_editor(state["editing"])
        imgui.separator()
        self.draw_palette()

    def add_check(self, condition: dict) -> None:
        if self.rows and templates.chain_mode(self.rows) == "any":
            self.rows[-1]["or"] = True
        self.rows.append(condition)
        self.state["editing"] = len(self.rows) - 1
        self.dirty = True

    # -- one check --

    def draw_check_editor(self, index: int) -> None:
        condition = self.rows[index]
        imgui.push_id(f"edit{index}")
        imgui.text_colored(semantic_color("accent"), templates.sentence(condition, self.describe))
        template = templates.match(condition)
        if template is not None:
            if len(template.who) > 1:
                who = templates.who_of(condition)
                imgui.text("About")
                for option in template.who:
                    if imgui.radio_button(templates.WHO[option], who == option) and who != option:
                        templates.set_who(condition, option)
                        self.dirty = True
            if templates.who_of(condition) == "reference":
                self.key_button(index, condition, "reference", "Placed reference", _REFERENCE)
            if template.picks_form:
                form = templates.form_of(condition)
                if imgui.button(f"{self.name(form) if form else 'Pick...'}##form", imgui.ImVec2(-1, 0)):
                    self.ask(f"form:{index}", template.signatures, form)
            self.draw_comparison(condition, template)
        else:
            imgui.text_wrapped("Set the condition, its parameters and comparison under Condition details.")
        imgui.set_next_item_open(template is None, imgui.Cond_.once)
        if imgui.collapsing_header("Condition details"):
            self.draw_raw_condition(condition, index)
        if imgui.small_button("Done"):
            self.state["editing"] = None
        imgui.pop_id()

    def draw_comparison(self, condition: dict, template: templates.Template) -> None:
        kind = template.kind
        if kind == templates.BOOL:
            yes = templates.truth(condition)
            if imgui.radio_button("is true", yes) and not yes:
                templates.set_truth(condition, True)
                self.dirty = True
            imgui.same_line()
            if imgui.radio_button("is false", not yes) and yes:
                templates.set_truth(condition, False)
                self.dirty = True
        elif kind == templates.SEX:
            female = bool(templates.sex_of(condition))
            if imgui.radio_button("male", not female) and female:
                templates.set_sex(condition, False)
                self.dirty = True
            imgui.same_line()
            if imgui.radio_button("female", female) and not female:
                templates.set_sex(condition, True)
                self.dirty = True
        elif kind == templates.DAY:
            imgui.set_next_item_width(160)
            changed, day = imgui.combo("Day", int(condition.get("value", 0)) % 7, list(templates.DAYS))
            if self.changed(changed):
                condition["value"] = float(day)
                condition["op"] = "=="
        elif kind == templates.PERCENT:
            imgui.set_next_item_width(-1)
            changed, percent = imgui.slider_int("##percent", int(condition.get("value", 50)), 1, 100, "%d%% chance")
            if self.changed(changed):
                condition["value"] = float(percent)
                condition["op"] = "<"
        else:
            words = [word for word, _ in templates.NUMBER_WORDS]
            current = next((i for i, (_, op) in enumerate(templates.NUMBER_WORDS) if op == condition.get("op")), 0)
            imgui.set_next_item_width(130)
            changed, current = imgui.combo("##cmp", current, words)
            if self.changed(changed):
                condition["op"] = templates.NUMBER_WORDS[current][1]
            imgui.same_line()
            imgui.set_next_item_width(-1)
            changed, number = imgui.input_float("##number", float(condition.get("value", 0)), 1.0, 10.0, "%g")
            if self.changed(changed):
                condition["value"] = number
        if template.extra == "stage":
            stage = (condition.get("params") or [None, None])[1] or {"raw": 0}
            imgui.set_next_item_width(160)
            changed, value = imgui.input_int("Stage", int(stage.get("raw", 0)))
            if self.changed(changed):
                condition["params"][1] = {"raw": max(0, value)}

    # -- palette --

    def draw_palette(self) -> None:
        imgui.text("Add a check")
        imgui.set_next_item_width(-1)
        _, self.state["filter"] = imgui.input_text_with_hint("##palette", "Search checks", self.state["filter"])
        needle = self.state["filter"].casefold()
        for category in templates.CATEGORIES:
            matches = [t for t in templates.TEMPLATES if t.category == category and
                       (not needle or needle in templates.TITLES[t.id].casefold() or needle in category.casefold())]
            if not matches:
                continue
            imgui.set_next_item_open(bool(needle), imgui.Cond_.once)
            if imgui.collapsing_header(f"{category}##palette"):
                for template in matches:
                    if imgui.selectable(f"{templates.TITLES[template.id]}##{template.id}", False,
                                        imgui.SelectableFlags_.allow_overlap)[0]:
                        self.add_check(templates.build(template))
                        if template.picks_form:
                            self.ask(f"form:{len(self.rows) - 1}", template.signatures)
                    imgui.same_line()
                    answer_mark(answered_by(template.function))
        for group, title in ((FALLOUT4, "All Fallout 4 conditions"), (FO76, "All Fallout 76 conditions")):
            functions = [f for f in FUNCTIONS if f.group == group and
                         (not needle or needle in f.name.casefold() or needle in f.note.casefold())]
            if not functions:
                continue
            imgui.set_next_item_open(bool(needle), imgui.Cond_.once)
            if imgui.collapsing_header(f"{title} ({len(functions)})##palette_{group}"):
                for function in functions:
                    if imgui.selectable(f"{function.name}##fn{function.id}", False, imgui.SelectableFlags_.allow_overlap)[0]:
                        self.add_function(function)
                    if function.note:
                        imgui.set_item_tooltip(function.note)
                    imgui.same_line()
                    answer_mark(answered_by(function.id))

    def add_function(self, function) -> None:
        self.add_check(new_condition(function.id))
        if function.params and function.params[0].kind == "form":
            self.ask(f"param:{len(self.rows) - 1}:0", function.params[0].signatures)

    # -- raw condition (Advanced) --

    def draw_raw_condition(self, condition: dict, index: int) -> None:
        imgui.set_next_item_width(-1)
        current = condition.get("function")
        if imgui.begin_combo("##function", function_label(current)):
            for group in (FALLOUT4, FO76):
                imgui.separator_text(group)
                for function in (f for f in FUNCTIONS if f.group == group):
                    if imgui.selectable(f"{function.name} ({function.id})", function.id == current)[0]:
                        condition["function"] = function.id
                        condition["params"] = [None, None]
                        self.dirty = True
                    if function.note:
                        imgui.set_item_tooltip(function.note)
            imgui.end_combo()
        known = BY_ID.get(condition.get("function"))
        if known and known.note:
            imgui.text_disabled(known.note)
        imgui.set_next_item_width(60)
        op = condition.get("op", "==")
        changed, op_index = imgui.combo("##op", OPS.index(op) if op in OPS else 0, list(OPS))
        if self.changed(changed):
            condition["op"] = OPS[op_index]
        imgui.same_line()
        if condition.get("global"):
            self.key_button(index, condition, "global", "vs global", ("GLOB",))
        else:
            imgui.set_next_item_width(90)
            changed, number = imgui.input_float("##value", float(condition.get("value", 0.0)))
            if self.changed(changed):
                condition["value"] = number
            imgui.same_line()
            if imgui.small_button("vs global"):
                self.ask(f"key:{index}:global", ("GLOB",))
        imgui.set_next_item_width(-1)
        run_on = condition.get("run_on", "subject")
        changed, run_index = imgui.combo("##run_on", RUN_ONS.index(run_on) if run_on in RUN_ONS else 0, _RUN_ON_NAMES)
        if self.changed(changed):
            condition["run_on"] = RUN_ONS[run_index]
        if condition.get("run_on") == "reference":
            self.key_button(index, condition, "reference", "Reference", _REFERENCE)
        changed, flag = imgui.checkbox("OR with the next check", bool(condition.get("or")))
        if self.changed(changed):
            condition["or"] = flag
        imgui.same_line()
        changed, flag = imgui.checkbox("Swap subject/target", bool(condition.get("swap")))
        if self.changed(changed):
            condition["swap"] = flag
        params = condition.setdefault("params", [None, None])
        while len(params) < 2:
            params.append(None)
        for slot, param in enumerate(known.params if known else ()):
            self.draw_param(index, params, slot, param)
        if imgui.small_button("Move up") and index:
            self.rows[index - 1], self.rows[index] = self.rows[index], self.rows[index - 1]
            self.state["editing"] = index - 1
            self.dirty = True
        imgui.same_line()
        if imgui.small_button("Move down") and index + 1 < len(self.rows):
            self.rows[index + 1], self.rows[index] = self.rows[index], self.rows[index + 1]
            self.state["editing"] = index + 1
            self.dirty = True

    def draw_param(self, index: int, params: list, slot: int, param: Param) -> None:
        imgui.push_id(f"param{slot}")
        current = params[slot]
        if param.kind == "form":
            key = current.get("form", "") if isinstance(current, dict) else ""
            if imgui.small_button(f"{param.name}: {self.name(key) if key else 'none'}"):
                self.ask(f"param:{index}:{slot}", param.signatures, key)
        elif param.kind in ("int", "sex"):
            raw = int(current.get("raw", 0)) if isinstance(current, dict) else 0
            imgui.set_next_item_width(140)
            if param.kind == "sex":
                changed, raw = imgui.combo(param.name, min(raw, 1), ["Male", "Female"])
            else:
                changed, raw = imgui.input_int(param.name, raw)
            if self.changed(changed) or current is None:
                params[slot] = {"raw": max(0, raw)}
        elif param.kind in ("condition_form", "region"):
            section = "condition_forms" if param.kind == "condition_form" else "regions"
            key = current.get("source", "") if isinstance(current, dict) else ""
            imgui.set_next_item_width(-1)
            if imgui.begin_combo(f"##{section}", f"{param.name}: {key or 'none'}"):
                if imgui.selectable("none", not key)[0]:
                    params[slot] = None
                    self.dirty = True
                for option in sorted(self.ctx.section_keys(section)):
                    if imgui.selectable(option, option == key)[0]:
                        params[slot] = {"source": option} if section == "condition_forms" else {"source": option, "form": option}
                        self.dirty = True
                imgui.end_combo()
        else:  # fo76_key: a SeventySix source key, never resolved in this load
            key = current.get("source", "") if isinstance(current, dict) else ""
            imgui.set_next_item_width(-1)
            changed, key = imgui.input_text_with_hint(f"##{param.name}", f"{param.name}: SeventySix.esm:000000", key)
            if self.changed(changed):
                params[slot] = {"source": key.strip()} if key.strip() else None
        imgui.pop_id()


def draw(label: str, node: dict, value: Any, ctx, id_path: str) -> tuple[bool, Any]:
    if label:
        imgui.text(label)
    editor = _Editor(value, ctx, id_path)
    editor.draw()
    return editor.dirty, (editor.rows if editor.dirty else value)
