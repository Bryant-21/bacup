"""Tests for block type categorization and the insert-block registry."""

import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from creation_lib.nif.schema import get_schema
from creation_lib.nif.types import categorize_block_type, BLOCK_CATEGORIES


def test_known_types_are_categorized():
    """Direct lookup in BLOCK_CATEGORIES works for all listed types."""
    assert categorize_block_type("BSFadeNode") == "Scene Nodes"
    assert categorize_block_type("BSTriShape") == "Geometry"
    assert categorize_block_type("BSLightingShaderProperty") == "Shader Properties"
    assert categorize_block_type("NiAlphaProperty") == "Alpha/Blending"
    assert categorize_block_type("BSXFlags") == "Extra Data"
    assert categorize_block_type("NiControllerManager") == "Animation Controllers"
    assert categorize_block_type("BSSkin::Instance") == "Skinning"
    assert categorize_block_type("bhkRigidBody") == "Collision"
    assert categorize_block_type("bhkRagdollConstraint") == "Constraints"
    assert categorize_block_type("NiParticleSystem") == "Particles"


def test_unknown_type_returns_other():
    """Types not in any category and no schema fall back to 'Other'."""
    assert categorize_block_type("SomeNewType") == "Other"


def test_all_hardcoded_types_exist_in_schema():
    """Every type listed in BLOCK_CATEGORIES should exist in nif.xml (with known exceptions)."""
    schema = get_schema()
    # NiPoint3Data is listed but doesn't exist in nif.xml — known gap
    known_missing = {"NiPoint3Data"}
    missing = []
    for cat_name, cat_info in BLOCK_CATEGORIES.items():
        for type_name in cat_info["types"]:
            if type_name not in schema.niobjects and type_name not in known_missing:
                missing.append((cat_name, type_name))
    assert missing == [], f"Types in BLOCK_CATEGORIES missing from schema: {missing}"


def test_no_duplicate_types_across_categories():
    """Each type should appear in at most one category."""
    seen = {}
    duplicates = []
    for cat_name, cat_info in BLOCK_CATEGORIES.items():
        for type_name in cat_info["types"]:
            if type_name in seen:
                duplicates.append((type_name, seen[type_name], cat_name))
            seen[type_name] = cat_name
    assert duplicates == [], f"Duplicate types across categories: {duplicates}"


def test_schema_fallback_categorizes_inherited_types():
    """Types not hardcoded but inheriting from a categorized type get categorized via schema."""
    schema = get_schema()
    # Find a NiNode subtype not in the hardcoded list
    node_subtypes_in_list = set(BLOCK_CATEGORIES["Scene Nodes"]["types"])
    for name, obj in schema.niobjects.items():
        if obj.abstract:
            continue
        if name in node_subtypes_in_list:
            continue
        if schema.is_subtype_of(name, "NiNode"):
            result = categorize_block_type(name, schema)
            assert result == "Scene Nodes", (
                f"{name} inherits NiNode but categorized as '{result}'"
            )
            break  # One example is enough


def test_bhk_prefix_fallback():
    """bhk* types not in the hardcoded list use prefix-based fallback.

    Exception: types that inherit from a non-collision categorized ancestor
    (e.g. bhkRagdollTemplate inherits NiExtraData) get that category instead.
    """
    schema = get_schema()
    hardcoded_collision = set(BLOCK_CATEGORIES["Collision"]["types"])
    hardcoded_constraints = set(BLOCK_CATEGORIES["Constraints"]["types"])
    # Collect categories that can override the bhk prefix via inheritance
    non_bhk_cats = set(BLOCK_CATEGORIES.keys()) - {"Collision", "Constraints"}

    for name, obj in schema.niobjects.items():
        if obj.abstract or not name.startswith("bhk"):
            continue
        if name in hardcoded_collision or name in hardcoded_constraints:
            continue
        result = categorize_block_type(name, schema)
        # If an ancestor is in a non-collision category, that takes priority
        hierarchy = schema.get_type_hierarchy(name)
        ancestor_override = False
        for ancestor in hierarchy[1:]:  # skip self
            for cat_name in non_bhk_cats:
                if ancestor in BLOCK_CATEGORIES.get(cat_name, {}).get("types", []):
                    ancestor_override = True
                    break
            if ancestor_override:
                break
        if ancestor_override:
            assert result in non_bhk_cats, f"{name} inherits non-bhk ancestor, got '{result}'"
        elif "Constraint" in name:
            assert result == "Constraints", f"{name} should be Constraints, got '{result}'"
        else:
            assert result == "Collision", f"{name} should be Collision, got '{result}'"


def test_get_all_block_types_covers_all_non_abstract():
    """get_all_block_types() should include every non-abstract type exactly once."""
    from ui.editor.block_ops import get_all_block_types
    schema = get_schema()

    cats = get_all_block_types()
    all_types = set()
    for types in cats.values():
        for type_name, _compat in types:
            assert type_name not in all_types, f"{type_name} appears in multiple categories"
            all_types.add(type_name)

    expected = {n for n, o in schema.niobjects.items() if not o.abstract}
    assert all_types == expected, (
        f"Missing: {expected - all_types}, Extra: {all_types - expected}"
    )


def test_fo4_compat_classification():
    """FO4-specific types should be classified as 'fo4', unrestricted as 'maybe'."""
    from ui.editor.block_ops import get_all_block_types
    cats = get_all_block_types()

    # Flatten to lookup
    compat_map = {}
    for types in cats.values():
        for type_name, compat in types:
            compat_map[type_name] = compat

    # BSTriShape has versions=['#SSE#', '#FO4#', '#F76#'] -> fo4
    assert compat_map["BSTriShape"] == "fo4"
    # NiNode has versions=[] -> maybe
    assert compat_map["NiNode"] == "maybe"
    # All compat values are valid
    valid = {"fo4", "maybe", "non_fo4"}
    for name, compat in compat_map.items():
        assert compat in valid, f"{name} has invalid compat '{compat}'"


def test_block_type_descriptions():
    """Block types with descriptions in nif.xml should be retrievable."""
    from ui.editor.block_ops import get_block_type_description
    desc = get_block_type_description("BSTriShape")
    assert desc == "Fallout 4 Tri Shape"
    desc2 = get_block_type_description("bhkBoxShape")
    assert desc2 == "A box."
    # Non-existent type returns empty
    assert get_block_type_description("FakeType") == ""
