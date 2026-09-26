from types import SimpleNamespace

import pytest

from bacup_lib.ui_contract import (
    render_as3_template,
    resolve_numbered_as3_call,
    resolve_numbered_name,
)


def test_numbered_name_requires_one_semantic_match():
    names = ["Menu_fla.WidgetPlain_10", "Menu_fla.Widget_777", "Other"]
    assert resolve_numbered_name(names, "Menu_fla.Widget_", "widget") == "Menu_fla.Widget_777"
    with pytest.raises(ValueError, match="found none"):
        resolve_numbered_name(names, "Menu_fla.Missing_", "widget")
    with pytest.raises(ValueError, match="Widget_1, Menu_fla.Widget_777"):
        resolve_numbered_name([*names, "Menu_fla.Widget_1"], "Menu_fla.Widget_", "widget")


def test_actionscript_template_requires_and_replaces_every_token():
    assert render_as3_template("new __CLASS__()", {"__CLASS__": "Menu_fla.Widget_777"}) == \
        "new Menu_fla.Widget_777()"
    with pytest.raises(RuntimeError, match="__MISSING__"):
        render_as3_template("source", {"__MISSING__": "value"})


def test_numbered_as3_call_resolves_generated_setter():
    runtime = SimpleNamespace(
        abc_disassemble=lambda *_: [{
            "method": "constructor",
            "code": ["  42  CallPropVoid __setProp_Widget_Menu_Widget_908 (0)"],
        }]
    )
    assert resolve_numbered_as3_call(
        runtime,
        b"movie",
        "Menu",
        "$constructor",
        "__setProp_Widget_Menu_Widget_",
        "widget setter",
    ) == "__setProp_Widget_Menu_Widget_908"
