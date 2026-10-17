from contextlib import closing

import pytest

from creation_lib.mod_manager.fomod import Environment, Fomod
from creation_lib.mod_manager.package import Package
from creation_lib.mod_manager.paths import ManagerError
from .test_fomod import group_xml


def flags_xml(*, order="Explicit", second_step=True):
    options = ''.join(f'<plugin name="{name}"><files><file source="{name}.txt"/></files><conditionFlags><flag name="shared"> {value} </flag></conditionFlags><typeDescriptor><type name="Required"/></typeDescriptor></plugin>'
                      for name, value in (("alpha", "first"), ("Zulu", "second")))
    next_step = '<installStep name="Next"><optionalFileGroups><group name="Later" type="SelectAny"><plugins><plugin name="Latest"><conditionFlags><flag name="shared">latest</flag></conditionFlags><typeDescriptor><type name="Required"/></typeDescriptor></plugin></plugins></group></optionalFileGroups></installStep>' if second_step else ''
    return f'<config><moduleName>Flag choices</moduleName><installSteps order="Explicit"><installStep name="First"><optionalFileGroups><group name="Shared" type="SelectAny"><plugins order="{order}">{options}</plugins></group></optionalFileGroups></installStep>{next_step}</installSteps></config>'.encode()


@pytest.mark.parametrize("order,expected", [("Explicit", "first"), ("Ascending", "second"), ("Descending", "first")])
def test_mo2_first_displayed_duplicate_flag_is_trimmed(order, expected):
    engine = Fomod(flags_xml(order=order, second_step=False), compatibility="mo2")
    assert engine.evaluate().flags == {"shared": expected}
    assert Fomod(flags_xml(order=order, second_step=False)).evaluate().flags["shared"] in {" first ", " second "}


def test_newest_visible_step_overrides_first_flag_from_earlier_step():
    engine = Fomod(flags_xml(), compatibility="mo2")
    assert engine.evaluate().flags == {"shared": "latest"}


@pytest.mark.parametrize("mode,expected", [("mo2", "Optional"), ("schema", "Required")])
def test_same_page_flag_type_policy_is_explicit(mode, expected):
    data = b'<config><moduleName>Same page</moduleName><installSteps><installStep name="One"><optionalFileGroups><group name="Group" type="SelectAny"><plugins order="Explicit"><plugin name="Set"><conditionFlags><flag name="on">yes</flag></conditionFlags><typeDescriptor><type name="Required"/></typeDescriptor></plugin><plugin name="Dynamic"><typeDescriptor><dependencyType><defaultType name="Optional"/><patterns><pattern><dependencies><flagDependency flag="on" value="yes"/></dependencies><type name="Required"/></pattern></patterns></dependencyType></typeDescriptor></plugin></plugins></group></optionalFileGroups></installStep></installSteps></config>'
    view = Fomod(data, compatibility=mode).evaluate()
    assert view.steps[0].groups[0].options[1].type == expected
    assert view.steps[0].groups[0].options[1].selected == (mode == "schema")


def test_mo2_select_all_forces_notusable_without_resolving_dynamic_dependencies(tmp_path):
    data = group_xml("SelectAll", ("Required", "NotUsable"))
    with closing(Package(tmp_path)) as package:
        # Reopen after generated fixture files exist; source inventory is immutable.
        assert not package.files
    (tmp_path / "file0.txt").write_bytes(b"zero")
    (tmp_path / "file1.txt").write_bytes(b"one")
    engine = Fomod(data, compatibility="mo2")
    with closing(Package(tmp_path)) as package:
        assert engine.plan(package) == {"file0.txt": "file0.txt", "file1.txt": "file1.txt"}
    dynamic = data.replace(b'<type name="NotUsable"/>', b'<dependencyType><defaultType name="NotUsable"/><patterns><pattern><dependencies><fileDependency file="Unknown.esp" state="Active"/></dependencies><type name="Optional"/></pattern></patterns></dependencyType>')
    view = Fomod(dynamic, compatibility="mo2").evaluate(environment=Environment(complete=False))
    assert not view.errors and all(option.selected for option in view.steps[0].groups[0].options)
    assert Fomod(data).evaluate().errors


@pytest.mark.parametrize("group", ["SelectAtLeastOne", "SelectExactlyOne"])
def test_mo2_single_choice_requirement_is_a_forced_group(group):
    view = Fomod(group_xml(group, ("NotUsable",)), compatibility="mo2").evaluate()
    assert not view.errors and view.steps[0].groups[0].type == "SelectAll"
    assert view.steps[0].groups[0].options[0].selected


def test_mo2_at_most_one_uses_last_recommended_radio_default():
    view = Fomod(group_xml("SelectAtMostOne", ("Recommended", "Recommended")), compatibility="mo2").evaluate()
    assert [option.selected for option in view.steps[0].groups[0].options] == [False, True]
    assert [option.selected for option in Fomod(group_xml("SelectAtMostOne", ("Recommended", "Recommended"))).evaluate().steps[0].groups[0].options] == [True, False]


def test_mo2_required_selection_prefers_optional_over_could_be_usable():
    view = Fomod(group_xml("SelectExactlyOne", ("CouldBeUsable", "Optional")), compatibility="mo2").evaluate()
    assert [option.selected for option in view.steps[0].groups[0].options] == [False, True]


@pytest.mark.parametrize("codec,header", [("latin-1", "iso-8859-1"), ("utf-16-le", "utf-16"), ("utf-16-be", "utf-16")])
def test_legacy_decoding_requires_explicit_compatibility_and_displays_notice(codec, header):
    data = f'<?xml version="1.0" encoding="{header}"?><config><moduleName>Copyright © fixture</moduleName></config>'.encode(codec)
    engine = Fomod(data, compatibility="mo2")
    assert engine.name == "Copyright © fixture" and engine.notices
    with pytest.raises(ManagerError):
        Fomod(data)


@pytest.mark.parametrize("mode", ["mo2", "schema"])
@pytest.mark.parametrize("codec", ["utf-8", "utf-16"])
def test_encoding_policy_never_permits_dtd_or_entities(mode, codec):
    data = '<!DOCTYPE config [<!ENTITY payload "never">]><config><moduleName>&payload;</moduleName></config>'.encode(codec)
    with pytest.raises(ManagerError, match="DTD and entity"):
        Fomod(data, compatibility=mode)


def test_unknown_policy_and_oversized_legacy_configuration_fail_closed():
    with pytest.raises(ManagerError, match="compatibility policy"):
        Fomod(b"<config/>", compatibility="unknown")
    with pytest.raises(ManagerError, match="2 MiB"):
        Fomod(b"a" * (2 * 1024**2 + 1), compatibility="mo2")


@pytest.mark.parametrize("version", [1, 2, 3, 4, 5])
def test_forced_group_policy_is_defined_for_every_normalized_schema(version):
    data = group_xml("SelectAll", ("Optional", "NotUsable")).decode()
    if version <= 3:
        groups = data[data.index("<optionalFileGroups"):data.index("</optionalFileGroups>") + len("</optionalFileGroups>")]
        if version <= 2:
            groups = groups.replace(' order="Explicit"', "")
        data = '<config><moduleName>Legacy policy</moduleName>' + groups + '</config>'
    data = data.replace('<config>', f'<config xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:noNamespaceSchemaLocation="ModConfig{version}.0.xsd">')
    engine = Fomod(data.encode(), compatibility="mo2")
    assert engine.version == version
    assert not engine.evaluate().errors
    assert all(option.selected for option in engine.evaluate().steps[0].groups[0].options)


def test_legacy_encoded_source_name_plans_the_exact_ordinary_fixture_file(tmp_path):
    (tmp_path / "©.txt").write_bytes(b"generated copyright fixture")
    data = '<config><moduleName>Legacy</moduleName><requiredInstallFiles><file source="©.txt" destination="result.txt"/></requiredInstallFiles></config>'.encode("latin-1")
    with closing(Package(tmp_path)) as package:
        engine = Fomod(data, compatibility="mo2")
        assert engine.encoding == "Latin-1"
        assert engine.plan(package) == {"result.txt": "©.txt"}
