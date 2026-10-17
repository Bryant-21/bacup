from contextlib import closing

import pytest

from creation_lib.mod_manager.package import Package
from .fomod_reference_corpus import cases, evaluate


@pytest.mark.parametrize("case", cases(), ids=lambda case: case.name)
def test_source_derived_semantics_and_documented_guard_differences(tmp_path, case):
    for name in ("a.txt", "b.txt"):
        (tmp_path / name).write_bytes(b"generated inert data")
    with closing(Package(tmp_path)) as package:
        for policy, expected in (("mo2", case.mo2_plan), ("schema", case.schema_plan)):
            result = evaluate(case, package, policy)
            assert result["plan"] == expected, (case.name, policy, result)
            assert bool(result["error"]) == (expected is None)
        if case.name == "v5-first-duplicate-flag":
            assert evaluate(case, package, "mo2")["flags"] == {"shared": "first"}
            assert evaluate(case, package, "schema")["flags"] == {"shared": " last "}
