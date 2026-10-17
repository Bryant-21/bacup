"""Original differential inputs, with explicit primary-source expectations.

No upstream implementation or schemas are bundled. The .NET XSD oracle runs
separately against pinned official schemas; MO2 semantic expectations below
are source-derived, not results from executing MO2's Qt installer.
"""
from dataclasses import dataclass
import json
from pathlib import Path

from creation_lib.mod_manager.fomod import Fomod
from creation_lib.mod_manager.package import Package
from creation_lib.mod_manager.paths import ManagerError


NMM_REVISION = "b6d77fc52cf9dc82c909399313ba562c938a8af2"
MO2_REVISION = "24f07cf7af5273efe52daa90c631a49e7e847c74"


@dataclass(frozen=True)
class Case:
    name: str
    version: int
    xml: str
    xsd_valid: bool
    mo2_plan: dict | None
    schema_plan: dict | None
    exception: str = ""


def config(version, content):
    return f'<config xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="ModConfig{version}.0.xsd"><moduleName>Generated reference fixture</moduleName>{content}</config>'


def plugin(version, name, source="a.txt", kind="Optional", flags=""):
    descriptor = f'<type name="{kind}"/>'
    descriptor = '<typeDescriptor>' + descriptor + '</typeDescriptor>'
    return f'<plugin name="{name}"><description>Original inert fixture</description><files><file source="{source}" destination="shared.txt"/></files>{flags}{descriptor}</plugin>'


def groups(version, kind, options, order="Explicit"):
    plugin_order = f' order="{order}"' if version >= 3 else ""
    content = f'<optionalFileGroups><group name="Generated group" type="{kind}"><plugins{plugin_order}>{options}</plugins></group></optionalFileGroups>'
    if version >= 4:
        content = '<installSteps order="Explicit"><installStep name="Generated step">' + content + '</installStep></installSteps>'
    return content


def cases():
    result = []
    for version in range(1, 6):
        base = '<requiredInstallFiles><file source="a.txt" destination="result.txt"/></requiredInstallFiles>'
        result.append(Case(f"v{version}-required", version, config(version, base), True, {"result.txt": "a.txt"}, {"result.txt": "a.txt"}))
        priority = '<requiredInstallFiles><file source="a.txt" destination="shared.txt" priority="-5"/><file source="b.txt" destination="shared.txt" priority="6"/></requiredInstallFiles>'
        result.append(Case(f"v{version}-priority", version, config(version, priority), True, {"shared.txt": "b.txt"}, {"shared.txt": "b.txt"}))
        tie = groups(version, "SelectAll", plugin(version, "B", "b.txt") + plugin(version, "A", "a.txt"))
        result.append(Case(f"v{version}-document-tie", version, config(version, tie), True, {"shared.txt": "a.txt"}, {"shared.txt": "a.txt"}))
        for group in ("SelectAny", "SelectAll", "SelectExactlyOne", "SelectAtLeastOne", "SelectAtMostOne"):
            payload = base.replace("result.txt", "baseline.txt") + groups(version, group, plugin(version, "A") + plugin(version, "B", "b.txt"))
            expected = {"shared.txt": "b.txt"} if group == "SelectAll" else ({"shared.txt": "a.txt"} if group in {"SelectExactlyOne", "SelectAtLeastOne"} else {})
            expected = {"baseline.txt": "a.txt", **expected}
            result.append(Case(f"v{version}-{group}", version, config(version, payload), True, expected, expected))
        invalid = groups(version, "UnsupportedGroup", plugin(version, "A"))
        result.append(Case(f"v{version}-invalid-enum", version, config(version, invalid), False, None, None))
        result.append(Case(f"v{version}-invalid-priority", version, config(version, base.replace('destination="result.txt"', 'destination="result.txt" priority="NaN"')), False, None, None))
        traversal = base.replace('destination="result.txt"', 'destination="../escape.txt"')
        result.append(Case(f"v{version}-path-guard", version, config(version, traversal), True, None, None, "Manager Windows path guard is stricter than XSD strings"))
        missing = base.replace('source="a.txt"', 'source="absent.txt"')
        result.append(Case(f"v{version}-missing-source", version, config(version, missing), True, None, None, "Missing payload is an explicit manager error; XSD does not inspect files"))
    for version in range(2, 6):
        forced = groups(version, "SelectAll", plugin(version, "A", kind="NotUsable") + plugin(version, "B", "b.txt"))
        result.append(Case(f"v{version}-forced-unusable", version, config(version, forced), True, {"shared.txt": "b.txt"}, None,
                           "MO2 forces SelectAll before dynamic availability; schema policy rejects unavailable choices"))
        recommended = groups(version, "SelectAtMostOne", plugin(version, "A", kind="Recommended") + plugin(version, "B", "b.txt", "Recommended"))
        result.append(Case(f"v{version}-last-recommended", version, config(version, recommended), True, {"shared.txt": "b.txt"}, {"shared.txt": "a.txt"},
                           "MO2 radio initialization retains the last recommended option; schema policy keeps the first"))
    flags = '<conditionFlags><flag name="shared"> first </flag></conditionFlags>'
    result.append(Case("v5-empty-selection-guard", 5, config(5, groups(5, "SelectAny", plugin(5, "A"))), True, None, None,
                       "Manager refuses a zero-file installation; XSD permits this selection"))
    options = plugin(5, "A", flags=flags) + plugin(5, "B", "b.txt", flags=flags.replace(" first ", " last "))
    result.append(Case("v5-first-duplicate-flag", 5, config(5, groups(5, "SelectAll", options)), True, {"shared.txt": "b.txt"}, {"shared.txt": "b.txt"},
                       "File tie follows XML document order; flag checks are independently asserted by semantic tests"))
    return tuple(result)


def evaluate(case, package, policy):
    try:
        engine = Fomod(case.xml.encode(), compatibility=policy)
        return {"plan": engine.plan(package), "flags": engine.evaluate().flags, "error": ""}
    except ManagerError as exc:
        return {"plan": None, "flags": {}, "error": str(exc)}


def export(root):
    root.mkdir(parents=True, exist_ok=True)
    package_root = root / "payload"
    package_root.mkdir(exist_ok=True)
    for name in ("a.txt", "b.txt"):
        (package_root / name).write_text("generated reference payload", encoding="utf-8")
    package = Package(package_root)
    results = []
    try:
        for case in cases():
            path = root / (case.name + ".xml")
            path.write_text(case.xml, encoding="utf-8")
            results.append({"name": case.name, "version": case.version, "path": str(path), "xsd_expected": case.xsd_valid,
                            "mo2_expected": case.mo2_plan, "schema_expected": case.schema_plan, "exception": case.exception,
                            "mo2": evaluate(case, package, "mo2"), "schema": evaluate(case, package, "schema")})
    finally:
        package.close()
    report = {"mo2_revision": MO2_REVISION, "schema_revision": NMM_REVISION, "official_engine_executed": False,
              "cases": results, "coverage": "Pinned XSD oracle plus source-derived MO2 semantic expectations; generated payloads only."}
    (root / "corpus.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(json.dumps({"cases": len(results), "manifest": str(root / "corpus.json")}))


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    export(parser.parse_args().output)
