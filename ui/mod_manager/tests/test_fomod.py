from contextlib import closing
import xml.etree.ElementTree as ET

import pytest

from creation_lib.mod_manager.fomod import Environment, Fomod, matches
from creation_lib.mod_manager.package import Package
from creation_lib.mod_manager.paths import ManagerError
from .fixtures import FOMOD_XML, make_package


def group_xml(kind, types=("Optional", "Optional")):
    options = ''.join(f'<plugin name="Option {i}"><files><file source="file{i}.txt" destination="file{i}.txt"/></files><typeDescriptor><type name="{type_}"/></typeDescriptor></plugin>'
                      for i, type_ in enumerate(types))
    return f'<config><moduleName>Groups</moduleName><installSteps order="Explicit"><installStep name="Step"><optionalFileGroups order="Explicit"><group name="Group" type="{kind}"><plugins order="Explicit">{options}</plugins></group></optionalFileGroups></installStep></installSteps></config>'.encode()


@pytest.mark.parametrize("group,selection,valid", [
    ("SelectAny", set(), True), ("SelectAny", {"0:0:0", "0:0:1"}, True),
    ("SelectAll", set(), True), ("SelectExactlyOne", set(), False),
    ("SelectExactlyOne", {"0:0:0"}, True), ("SelectExactlyOne", {"0:0:0", "0:0:1"}, False),
    ("SelectAtMostOne", set(), True), ("SelectAtMostOne", {"0:0:0", "0:0:1"}, False),
    ("SelectAtLeastOne", set(), False), ("SelectAtLeastOne", {"0:0:0", "0:0:1"}, True),
])
def test_all_group_cardinalities(group, selection, valid):
    evaluated = Fomod(group_xml(group)).evaluate({"0:0": selection})
    assert (not evaluated.errors) is valid
    if group == "SelectAll":
        assert all(option.selected for option in evaluated.steps[0].groups[0].options)


def test_required_notusable_and_recommended_defaults():
    engine = Fomod(group_xml("SelectAny", ("Required", "NotUsable", "Recommended", "CouldBeUsable")))
    evaluation = engine.evaluate()
    assert [item.selected for item in evaluation.steps[0].groups[0].options] == [True, False, True, False]
    assert engine.evaluate({"0:0": {"0:0:1"}}).errors
    assert engine.evaluate({"0:0": {"0:0:3"}}).steps[0].groups[0].options[0].selected
    assert Fomod(group_xml("SelectAll", ("Required", "NotUsable"))).evaluate().errors


def test_incomplete_environment_requires_explicit_absent_file_state():
    node = ET.fromstring('<fileDependency file="BaseGame.esm" state="Missing"/>')
    with pytest.raises(ManagerError, match="unknown"):
        matches(node, Environment(complete=False), {})
    assert matches(node, Environment({"BaseGame.esm": "Missing"}, complete=False), {})


def test_inconsistent_legacy_schema_and_non_utf8_xml_are_rejected():
    with pytest.raises(ManagerError, match="root construct"):
        Fomod(FOMOD_XML.replace(b"ModConfig5.0.xsd", b"ModConfig3.0.xsd"))
    with pytest.raises(ManagerError, match="UTF-8"):
        Fomod(b'<config><moduleName>\xa9</moduleName></config>')


def test_default_choices_visibility_dynamic_types_flags_and_priority(tmp_path):
    package = Package(make_package(tmp_path))
    engine = Fomod(FOMOD_XML)
    low = engine.evaluate()
    assert len(low.steps) == 1
    assert low.flags == {"quality": "standard"}
    assert engine.plan(package)["textures/shared.txt"] == "standard/shared.txt"
    choice = {"0:0": {"0:0:1"}}
    context = Environment({"FIXTURE.ESP": "Active"})
    high = engine.evaluate(choice, context)
    assert len(high.steps) == 2
    assert high.steps[1].groups[0].options[0].type == "Required"
    plan = engine.plan(package, choice, context)
    assert plan["textures/shared.txt"] == "patch.txt"
    assert plan["textures/detail.txt"] == "detail.txt"
    assert plan["readme.txt"] == "readme.txt"
    assert plan["textures/extra.txt"] == "high/extra.txt"
    back = engine.plan(package, {"0:0": {"0:0:0"}, "1:0": {"1:0:0"}}, context)
    assert "textures/detail.txt" not in back
    package.close()


def test_nested_file_flag_and_version_dependencies():
    node = ET.fromstring('<dependencies operator="Or"><fileDependency file="Missing.esp" state="Active"/><dependencies><flagDependency flag="x" value="yes"/><gameDependency version="1.2.0"/><fommDependency version="0.13"/><fileDependency file="Disabled.esp" state="Inactive"/></dependencies></dependencies>')
    assert matches(node, Environment({"disabled.ESP": "Inactive"}, "1.2", "0.14"), {"x": "yes"})
    assert not matches(node, Environment({"disabled.esp": "Inactive"}, "1.1", "0.14"), {"x": "yes"})
    with pytest.raises(ManagerError, match="explicit version"):
        matches(node, Environment({"Disabled.esp": "Inactive"}), {"x": "yes"})


def test_dependency_unknowns_only_block_undecidable_expressions():
    unknown = '<fileDependency file="Unknown.esp" state="Active"/>'
    true = '<flagDependency flag="known" value="yes"/>'
    false = '<flagDependency flag="known" value="no"/>'
    environment = Environment(complete=False)
    for first, second in ((unknown, true), (true, unknown)):
        assert matches(ET.fromstring(f'<dependencies operator="Or">{first}{second}</dependencies>'), environment, {"known": "yes"})
    for first, second in ((unknown, false), (false, unknown)):
        assert not matches(ET.fromstring(f'<dependencies>{first}{second}</dependencies>'), environment, {"known": "yes"})
    with pytest.raises(ManagerError, match="unknown"):
        matches(ET.fromstring(f'<dependencies>{unknown}{true}</dependencies>'), environment, {"known": "yes"})


def test_all_matching_conditionals_and_dynamic_first_match(tmp_path):
    package = Package(make_package(tmp_path))
    data = FOMOD_XML.replace(b'</patterns></dependencyType>', b'<pattern><dependencies><fileDependency file="Fixture.esp" state="Active"/></dependencies><type name="NotUsable"/></pattern></patterns></dependencyType>')
    engine = Fomod(data)
    plan = engine.plan(package, {"0:0": {"0:0:1"}}, Environment({"Fixture.esp": "Active"}))
    assert "readme.txt" in plan and plan["textures/shared.txt"] == "patch.txt"
    assert engine.evaluate({"0:0": {"0:0:1"}}, Environment({"Fixture.esp": "Active"})).steps[1].groups[0].options[0].type == "Required"
    package.close()


def test_always_install_and_install_if_usable(tmp_path):
    data = group_xml("SelectAny", ("NotUsable", "Optional")).replace(b'source="file0.txt"', b'source="file0.txt" alwaysInstall="true"').replace(b'source="file1.txt"', b'source="file1.txt" installIfUsable="1"')
    folder = tmp_path / "package"
    folder.mkdir()
    (folder / "file0.txt").write_text("always")
    (folder / "file1.txt").write_text("usable")
    package = Package(folder)
    assert set(Fomod(data).plan(package, {"0:0": set()})) == {"file0.txt", "file1.txt"}
    not_usable = data.replace(b'name="Optional"', b'name="NotUsable"')
    assert set(Fomod(not_usable).plan(package, {"0:0": set()})) == {"file0.txt"}


def test_nested_package_root_case_insensitive_source_and_file_rename(tmp_path):
    folder = tmp_path / "package"
    (folder / "Wrapper" / "fomod").mkdir(parents=True)
    (folder / "Wrapper" / "Sources").mkdir()
    (folder / "Wrapper" / "Sources" / "A.txt").write_text("payload")
    data = b'<config><moduleName>Name</moduleName><requiredInstallFiles><file source="sources/a.TXT" destination="Renamed.txt"/></requiredInstallFiles></config>'
    (folder / "Wrapper" / "fomod" / "ModuleConfig.xml").write_bytes(data)
    package = Package(folder)
    assert Fomod(data).plan(package) == {"Renamed.txt": "Wrapper/Sources/A.txt"}


def test_ordering_and_equal_priority_are_deterministic(tmp_path):
    data = group_xml("SelectAll").replace(b'plugins order="Explicit"', b'plugins order="Descending"')
    evaluation = Fomod(data).evaluate()
    assert [option.name for option in evaluation.steps[0].groups[0].options] == ["Option 1", "Option 0"]
    folder = tmp_path / "folder"
    folder.mkdir()
    for name in ("a", "b"):
        (folder / name).write_text(name)
    package = Package(folder)
    data = b'<config><moduleName>Name</moduleName><requiredInstallFiles><file source="a" destination="same" priority="2"/><file source="b" destination="SAME" priority="2"/></requiredInstallFiles></config>'
    assert Fomod(data).plan(package) == {"SAME": "b"}


@pytest.mark.parametrize("data, message", [
    (b'<!DOCTYPE config [<!ENTITY x "evil">]><config><moduleName>&x;</moduleName></config>', "DTD"),
    (b'<config><moduleName>Name</moduleName><script/></config>', "Unexpected"),
    (b'<config><moduleName>Name</moduleName><moduleDependencies><arbitraryDependency version="1"/></moduleDependencies></config>', "Unexpected"),
    (b'<config><moduleName>Name</moduleName><requiredInstallFiles><file source="../outside"/></requiredInstallFiles></config>', "Unsafe"),
    (b'<config><moduleName>Name</moduleName><requiredInstallFiles><file source="a" priority="bad"/></requiredInstallFiles></config>', "integer"),
    (b'<config><moduleName>Name</moduleName><requiredInstallFiles><file source="a" alwaysInstall="yes"/></requiredInstallFiles></config>', "boolean"),
    (b'<config><moduleName>Name</moduleName><moduleName>Duplicate</moduleName></config>', "one"),
    (b'<config><moduleName>Name</moduleName><moduleDependencies operator="X"/></config>', "operator"),
    (b'<config><moduleName>Name</moduleName><unknown/></config>', "Unexpected"),
    (b'<config', "Invalid"),
])
def test_invalid_and_unsupported_configs_fail_closed(data, message):
    with pytest.raises(ManagerError, match=message):
        Fomod(data)


def test_failed_module_dependency_and_missing_selected_source(tmp_path):
    data = b'<config><moduleName>Name</moduleName><moduleDependencies><fileDependency file="Required.esp" state="Active"/></moduleDependencies></config>'
    with pytest.raises(ManagerError, match="not satisfied"):
        Fomod(data).evaluate()
    package = Package(make_package(tmp_path))
    data = b'<config><moduleName>Name</moduleName><requiredInstallFiles><file source="missing" destination="target"/></requiredInstallFiles></config>'
    with pytest.raises(ManagerError, match="missing"):
        Fomod(data).plan(package)


@pytest.mark.parametrize("version", [1, 2, 3, 4, 5])
def test_versioned_declarative_file_defaults_and_explicit_root(tmp_path, version):
    folder = tmp_path / "package"
    (folder / "Wrapper/fomod").mkdir(parents=True)
    (folder / "Wrapper/Textures").mkdir()
    (folder / "Wrapper/Textures/A.txt").write_text("payload")
    data = f'<config xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="ModConfig{version}.0.xsd"><moduleName>Fixture</moduleName><requiredInstallFiles><file source="Textures/A.txt"/><folder source="Textures" destination=""/></requiredInstallFiles></config>'.encode()
    (folder / "Wrapper/fomod/ModuleConfig.xml").write_bytes(data)
    package = Package(folder)
    try:
        engine = Fomod(data)
        assert engine.version == version
        assert engine.plan(package) == {"Textures/A.txt": "Wrapper/Textures/A.txt", "A.txt": "Wrapper/Textures/A.txt"}
        omitted_folder = data.replace(b'<file source="Textures/A.txt"/><folder source="Textures" destination=""/>', b'<folder source="Textures"/>')
        assert Fomod(omitted_folder).plan(package) == {"Textures/A.txt": "Wrapper/Textures/A.txt"}
    finally:
        package.close()


@pytest.mark.parametrize("version", [1, 2, 3])
def test_legacy_single_page_dependencies_dynamic_types_flags_and_order(tmp_path, version):
    module_tag = "moduleDependancies" if version == 1 else "moduleDependencies"
    file_tag = "fileDependancy" if version == 1 else "fileDependency"
    game_tag = "falloutDependancy" if version == 1 else "falloutDependency"
    dependency_tag = "dependancies" if version == 1 else "dependencies"
    item_tag = "dependancy" if version == 1 else "fileDependency"
    type_tag = "dependancyType" if version == 1 else "dependencyType"
    module_file = f'<{file_tag} file="Fixture.esp"' + (' state="Active"' if version == 3 else '') + '/>'
    flags = '<conditionFlags><flag name="choice">yes</flag></conditionFlags>' if version >= 2 else ''
    conditional = '<conditionalFileInstalls><patterns><pattern><dependencies><flagDependency flag="choice" value="yes"/></dependencies><files><file source="extra.txt"/></files></pattern></patterns></conditionalFileInstalls>' if version >= 2 else ''
    options = f'<plugin name="Zeta"><description>Fixture</description><files><file source="z.txt"/></files><typeDescriptor><{type_tag}><defaultType name="NotUsable"/><patterns><pattern><{dependency_tag}><{item_tag} file="Fixture.esp" state="Active"/></{dependency_tag}><type name="Recommended"/></pattern></patterns></{type_tag}></typeDescriptor>{flags}</plugin><plugin name="Alpha"><description>Fixture</description><files><file source="a.txt"/></files><typeDescriptor><type name="Optional"/></typeDescriptor></plugin>'
    group = '<group name="B" type="SelectAny"><plugins>' + options + '</plugins></group><group name="A" type="SelectAny"><plugins><plugin name="Only"><description>Fixture</description><files/><typeDescriptor><type name="Optional"/></typeDescriptor></plugin></plugins></group>'
    data = f'<config xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="ModConfig{version}.0.xsd"><moduleName>Old fixture</moduleName><{module_tag}>{module_file}<{game_tag} version="1.2"/></{module_tag}><optionalFileGroups>{group}</optionalFileGroups>{conditional}</config>'.encode()
    folder = tmp_path / "package"
    folder.mkdir()
    for name in ("a.txt", "z.txt", "extra.txt"):
        (folder / name).write_text(name)
    package = Package(folder)
    try:
        engine = Fomod(data)
        environment = Environment({"Fixture.esp": "Active"}, "1.3")
        evaluation = engine.evaluate(environment=environment)
        assert len(evaluation.steps) == 1
        assert [group.name for group in evaluation.steps[0].groups] == (["A", "B"] if version == 3 else ["B", "A"])
        group_view = next(group for group in evaluation.steps[0].groups if group.name == "B")
        assert [option.name for option in group_view.options] == ["Alpha", "Zeta"]
        assert group_view.options[1].selected and group_view.options[1].type == "Recommended"
        assert set(engine.plan(package, environment=environment)) == ({"z.txt", "extra.txt"} if version >= 2 else {"z.txt"})
    finally:
        package.close()


@pytest.mark.parametrize("node,identifier", [("foseDependency", "fose"), ("nvseDependency", "nvse"), ("skseDependency", "skse"), ("f4seDependency", "f4se")])
def test_script_extender_checks_require_explicit_versions(node, identifier):
    data = f'<config><moduleName>Fixture</moduleName><moduleDependencies><{node} version="1.2"/></moduleDependencies></config>'.encode()
    engine = Fomod(data)
    with pytest.raises(ManagerError, match="explicit version"):
        engine.evaluate()
    assert not engine.evaluate(environment=Environment(extender_versions={identifier: "1.3"})).errors
    with pytest.raises(ManagerError, match="not satisfied"):
        engine.evaluate(environment=Environment(extender_versions={identifier: "1.1"}))


def test_legacy_invalid_extensions_and_script_execution_are_rejected(tmp_path):
    with pytest.raises(ManagerError, match="FOMOD 1.0"):
        Fomod(b'<config xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="ModConfig1.0.xsd"><moduleName>Fixture</moduleName><conditionalFileInstalls/></config>')
    with pytest.raises(ManagerError):
        Fomod(b'<config><moduleName>Fixture</moduleName><moduleDependencies><script command="run"/></moduleDependencies></config>')
    folder = tmp_path / "scripted"
    (folder / "fomod").mkdir(parents=True)
    (folder / "fomod/script.cs").write_text("fixture code must never execute")
    with pytest.raises(ManagerError, match="Executable"):
        Package(folder)


@pytest.mark.parametrize("version", [2, 3, 4, 5])
def test_same_page_flags_recompute_dynamic_types_and_unknown_branches(tmp_path, version):
    first = '<plugin name="A toggle"><files><file source="a.txt"/></files><conditionFlags><flag name="feature">on</flag></conditionFlags><typeDescriptor><type name="Recommended"/></typeDescriptor></plugin>'
    second = '<plugin name="B patch"><files><file source="b.txt"/></files><typeDescriptor><dependencyType><defaultType name="NotUsable"/><patterns><pattern><dependencies operator="Or"><fileDependency file="Unknown.esp" state="Active"/><flagDependency flag="feature" value="on"/></dependencies><type name="Required"/></pattern></patterns></dependencyType></typeDescriptor></plugin>'
    groups = '<optionalFileGroups><group name="Options" type="SelectAny"><plugins>' + first + second + '</plugins></group></optionalFileGroups>'
    options = groups if version < 4 else '<installSteps><installStep name="Step">' + groups + '</installStep></installSteps>'
    data = f'<config xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="ModConfig{version}.0.xsd"><moduleName>Fixture</moduleName><requiredInstallFiles><file source="base.txt"/></requiredInstallFiles>{options}</config>'.encode()
    folder = tmp_path / "package"
    folder.mkdir()
    for name in ("base.txt", "a.txt", "b.txt"):
        (folder / name).write_text(name)
    package = Package(folder)
    try:
        engine = Fomod(data)
        environment = Environment(complete=False)
        assert set(engine.plan(package, environment=environment)) == {"base.txt", "a.txt", "b.txt"}
        environment.files["unknown.esp"] = "Missing"
        assert set(engine.plan(package, {"0:0": set()}, environment)) == {"base.txt"}
        assert engine.evaluate({"0:0": set()}, environment).steps[0].groups[0].options[1].type == "NotUsable"
    finally:
        package.close()


def test_cyclic_same_page_type_flags_fail_explicitly():
    data = b'<config><moduleName>Cycle</moduleName><installSteps><installStep name="Step"><optionalFileGroups><group name="Group" type="SelectAny"><plugins><plugin name="Oscillating"><conditionFlags><flag name="self">on</flag></conditionFlags><typeDescriptor><dependencyType><defaultType name="Required"/><patterns><pattern><dependencies><flagDependency flag="self" value="on"/></dependencies><type name="NotUsable"/></pattern></patterns></dependencyType></typeDescriptor></plugin></plugins></group></optionalFileGroups></installStep></installSteps></config>'
    with pytest.raises(ManagerError, match="cyclic"):
        Fomod(data).evaluate()


@pytest.mark.parametrize("codec", ["utf-16", "utf-16-be"])
def test_utf16_declarative_xml_and_entity_guards(codec):
    text = '<?xml version="1.0" encoding="utf-16"?><config><moduleName>Unicode fixture é</moduleName></config>'
    prefix = b"\xfe\xff" if codec == "utf-16-be" else b""
    assert Fomod(prefix + text.encode(codec)).name == "Unicode fixture é"
    hostile = '<!DOCTYPE config [<!ENTITY x "expansion">]><config><moduleName>&x;</moduleName></config>'
    with pytest.raises(ManagerError, match="DTD"):
        Fomod(prefix + hostile.encode(codec))


def test_oversized_version_input_is_an_explicit_installer_error():
    node = ET.fromstring('<gameDependency version="1"/>')
    with pytest.raises(ManagerError, match="Unsupported version"):
        matches(node, Environment(game_version="9" * 5000), {})


def test_unset_flag_empty_value_selects_fallback_files(tmp_path):
    folder = tmp_path / "package"
    folder.mkdir()
    (folder / "fallback.txt").write_bytes(b"generated fallback")
    data = b'<config><moduleName>Fallback</moduleName><conditionalFileInstalls><patterns><pattern><dependencies><flagDependency flag="unset" value=""/></dependencies><files><file source="fallback.txt"/></files></pattern></patterns></conditionalFileInstalls></config>'
    with closing(Package(folder)) as package:
        assert Fomod(data).plan(package) == {"fallback.txt": "fallback.txt"}
    node = ET.fromstring('<flagDependency flag="unset" value=""/>')
    assert matches(node, Environment(), {}) and matches(node, Environment(), {"unset": ""})
    assert not matches(node, Environment(), {"unset": "selected"})


@pytest.mark.parametrize("tag", ["file", "folder"])
@pytest.mark.parametrize("wrapper", ["", "wrapper/"])
def test_empty_source_none_sentinel_never_copies_unselected_payload_or_metadata(tmp_path, tag, wrapper):
    folder = tmp_path / "package"
    (folder / wrapper / "fomod").mkdir(parents=True)
    (folder / wrapper / "base.txt").write_bytes(b"base")
    (folder / wrapper / "unselected.txt").write_bytes(b"must not be selected")
    data = f'<config><moduleName>None sentinel</moduleName><requiredInstallFiles><file source="base.txt"/><{tag} source="" destination=""/></requiredInstallFiles></config>'.encode()
    (folder / wrapper / "fomod" / "ModuleConfig.xml").write_bytes(data)
    with closing(Package(folder)) as package:
        assert Fomod(data).plan(package) == {"base.txt": wrapper + "base.txt"}


def test_equal_priority_uses_xml_rule_order_even_when_display_is_sorted(tmp_path):
    folder = tmp_path / "package"
    folder.mkdir()
    for source in ("a.txt", "b.txt"):
        (folder / source).write_text(source)
    plugins = ''.join(f'<plugin name="{name}"><files><file source="{source}" destination="shared.txt"/></files><typeDescriptor><type name="Required"/></typeDescriptor></plugin>' for name, source in (("B", "b.txt"), ("A", "a.txt")))
    data = f'<config><moduleName>Order</moduleName><installSteps><installStep name="Step"><optionalFileGroups><group name="Group" type="SelectAny"><plugins>{plugins}</plugins></group></optionalFileGroups></installStep></installSteps></config>'.encode()
    engine = Fomod(data)
    assert [option.name for option in engine.evaluate().steps[0].groups[0].options] == ["A", "B"]
    with closing(Package(folder)) as package:
        assert engine.plan(package) == {"shared.txt": "a.txt"}


def test_cancellation_during_fixed_point_evaluation_does_not_wait_for_full_pass(monkeypatch):
    import threading
    from creation_lib.mod_manager.paths import Cancelled
    engine, cancel = Fomod(group_xml("SelectAny", ("Required",) * 100)), threading.Event()
    calls = []
    def stop(plugin):
        calls.append(plugin)
        cancel.set()
    monkeypatch.setattr(engine, "_checkpoint", stop)
    with pytest.raises(Cancelled):
        engine.evaluate(cancel=cancel)
    assert len(calls) == 1


def test_legacy_normalization_preserves_original_xml_tie_order(tmp_path):
    folder = tmp_path / "package"
    folder.mkdir()
    for source in ("a.txt", "b.txt"):
        (folder / source).write_text(source)
    data = b'<config xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="XmlScript3.0.xsd"><moduleName>Legacy</moduleName><optionalFileGroups><group name="Group" type="SelectAny"><plugins><plugin name="Option"><files><file source="a.txt" destination="shared.txt"/></files><typeDescriptor><type name="Required"/></typeDescriptor></plugin></plugins></group></optionalFileGroups><conditionalFileInstalls><patterns><pattern><dependencies><flagDependency flag="unset" value=""/></dependencies><files><file source="b.txt" destination="shared.txt"/></files></pattern></patterns></conditionalFileInstalls></config>'
    with closing(Package(folder)) as package:
        assert Fomod(data).plan(package) == {"shared.txt": "b.txt"}


@pytest.mark.parametrize("value", [" true ", " 1\t", " false\n", " 0 "])
def test_schema_boolean_whitespace_collapses(value):
    data = group_xml("SelectAny").replace(b'source="file0.txt"', f'source="file0.txt" alwaysInstall="{value}"'.encode())
    assert not Fomod(data).evaluate().errors
