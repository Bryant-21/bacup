"""Regression tests for the @drop-member / @drop-property patch directives."""

from bacup_lib.workflows.unified import _merge_script_method_patches

SKELETON = """Scriptname ExampleScript Extends ObjectReference

Int Property StageToSet Auto Mandatory
Int Property KeepMe Auto

Event OnSimpleNetworkStateSet()
    DoThing()
EndEvent

Event OnLoad()
    DoThing()
EndEvent

Auto State Closed
    Event OnEnterFurniture(ObjectReference akActionRef)
        PlayAnimation("close")
    EndEvent

    Event OnBeginState(String asOldState)
        PlayAnimation("close")
    EndEvent
EndState
"""


def test_drop_member_removes_top_level_event():
    merged = _merge_script_method_patches(
        SKELETON, "; @drop-member OnSimpleNetworkStateSet\n"
    )
    assert "OnSimpleNetworkStateSet" not in merged
    assert "Event OnLoad()" in merged


def test_drop_member_removes_event_inside_named_state():
    merged = _merge_script_method_patches(SKELETON, "; @drop-member OnEnterFurniture\n")
    assert "OnEnterFurniture" not in merged
    assert "Event OnBeginState(String asOldState)" in merged
    assert "Auto State Closed" in merged


def test_drop_property_removes_only_the_named_declaration():
    merged = _merge_script_method_patches(SKELETON, "; @drop-property StageToSet\n")
    assert "Property StageToSet" not in merged
    assert "Int Property KeepMe Auto" in merged
    assert "Scriptname ExampleScript" in merged


def test_directives_compose_with_ordinary_member_replacement():
    patch = """; @drop-member OnSimpleNetworkStateSet
; @drop-property StageToSet

Event OnLoad()
    DoOtherThing()
EndEvent
"""
    merged = _merge_script_method_patches(SKELETON, patch)
    assert "OnSimpleNetworkStateSet" not in merged
    assert "Property StageToSet" not in merged
    assert "DoOtherThing()" in merged
    assert merged.count("Event OnLoad()") == 1


def test_patch_without_directives_or_members_is_a_no_op():
    assert _merge_script_method_patches(SKELETON, "; just a comment\n") == SKELETON
