"""Reviewed repairs keep declarations, payloads and destinations explicit."""
from contextlib import closing

import pytest

from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.fomod import Fomod
from creation_lib.mod_manager import installer_repair, installer_wizard
from creation_lib.mod_manager.library import Library
from creation_lib.mod_manager.package import Package
from creation_lib.mod_manager.paths import Cancelled, ManagerError
from .test_controller import finish


def repair_package(directory, *, conflict=False):
    (directory / "fomod").mkdir(parents=True)
    (directory / "replacement.txt").write_bytes(b"generated replacement")
    (directory / "alternative").mkdir()
    (directory / "alternative" / "file.txt").write_bytes(b"generated folder replacement")
    options = ''
    if conflict:
        plugins = ''.join(f'<plugin name="{name}"><typeDescriptor><type name="Required"/></typeDescriptor></plugin>' for name in ("First", "Second"))
        options = f'<installSteps><installStep name="Conflict"><optionalFileGroups><group name="Required radio" type="SelectExactlyOne"><plugins>{plugins}</plugins></group></optionalFileGroups></installStep></installSteps>'
    xml = f'<config><moduleName>Repair fixture</moduleName><requiredInstallFiles><file source="missing.txt" destination="textures/"/><folder source="missing-folder" destination="meshes"/></requiredInstallFiles>{options}</config>'
    (directory / "fomod" / "ModuleConfig.xml").write_text(xml, encoding="utf-8")
    return directory


def test_file_folder_repair_keeps_destinations_and_source_unchanged(tmp_path):
    root = repair_package(tmp_path / "package")
    before = {p: p.read_bytes() for p in root.rglob("*") if p.is_file()}
    with closing(Package(root)) as package:
        fomod = Fomod(package.read_config())
        issues = installer_repair.missing(package, fomod.evaluate())
        assert {issue.source for issue in issues} == {"missing.txt", "missing-folder"}
        plan = fomod.plan(package, repairs={"missing.txt": "replacement.txt", "missing-folder": "alternative"})
        assert plan == {"textures/missing.txt": "replacement.txt", "meshes/file.txt": "alternative/file.txt"}
        package.validate_sources(plan.values())
    assert {p: p.read_bytes() for p in before} == before


@pytest.mark.parametrize("binding", [
    {"missing.txt": "../outside"}, {"missing.txt": "fomod/ModuleConfig.xml"},
    {"missing.txt": "alternative"}, {"missing-folder": "replacement.txt"},
    {"missing.txt": "not-there.txt"}, {"available.txt": "replacement.txt"},
])
def test_invalid_or_stale_rebindings_are_rejected(tmp_path, binding):
    with closing(Package(repair_package(tmp_path / "package"))) as package:
        fomod = Fomod(package.read_config())
        with pytest.raises(ManagerError):
            fomod.plan(package, repairs=binding)


def test_required_radio_conflict_blocks_next_and_plan_without_dropping_requirements(tmp_path):
    with closing(Package(repair_package(tmp_path / "package", conflict=True))) as package:
        fomod = Fomod(package.read_config())
        evaluation = fomod.evaluate()
        assert len([o for o in evaluation.steps[0].groups[0].options if o.selected]) == 2
        assert not installer_wizard.can_advance(evaluation, 0)
        with pytest.raises(ManagerError, match="SelectExactlyOne"):
            fomod.plan(package, repairs={"missing.txt": "replacement.txt", "missing-folder": "alternative"})


@pytest.fixture
def manager(tmp_path):
    m = ManagerController()
    m.library = Library(tmp_path / "library", create=True)
    m.profile_id = m.library.create_profile("Local")
    m.refresh()
    m.prepare_install(repair_package(tmp_path / "package"))
    finish(m)
    m.install_repairs = {"missing.txt": "replacement.txt", "missing-folder": "alternative"}
    yield m
    m.cleanup()


@pytest.mark.parametrize("mutation", ["repair", "choices", "environment", "mapping", "addition", "plan", "target"])
def test_repair_review_configuration_cannot_change_before_install(manager, mutation):
    m = manager
    m.review_install()
    finish(m)
    if mutation == "repair":
        m.install_repairs["missing.txt"] = "alternative/file.txt"
    elif mutation == "choices":
        m.choices["new-group"] = {"new-option"}
    elif mutation == "environment":
        m.prepared.environment.complete = True
    elif mutation == "mapping":
        m.install_mapping["textures/missing.txt"] = "other.txt"
    elif mutation == "addition":
        m.install_additions["extra.txt"] = "replacement.txt"
    elif mutation == "target":
        m.target_id = "different-target"
    else:
        m.install_plan["unreviewed.txt"] = "replacement.txt"
    with pytest.raises(ManagerError, match="changed after review"):
        m.install("Blocked changed plan")
    assert not any((m.library.root / "mods").iterdir())


@pytest.mark.parametrize("change", ["delete", "payload", "config"])
def test_package_change_requires_retry_and_keeps_review_unpublished(manager, change):
    m = manager
    file = m.prepared.package.path / ("fomod/ModuleConfig.xml" if change == "config" else "replacement.txt")
    if change == "delete":
        file.unlink()
    else:
        file.write_bytes(file.read_bytes() + b"generated later change")
    m.review_install()
    assert isinstance(m._future.exception(timeout=10), ManagerError)
    m.poll()
    assert m.install_plan is None and m.job_state == "failed"
    assert not any((m.library.root / "mods").iterdir())


@pytest.mark.parametrize("outcome", ["cancel", "fail", "stale"])
def test_repaired_install_staging_rollback_preserves_original_package(manager, monkeypatch, outcome):
    m = manager
    original = {p: p.read_bytes() for p in m.prepared.package.path.rglob("*") if p.is_file()}
    m.review_install()
    finish(m)
    def fault(phase):
        if phase == "prepared":
            if outcome == "cancel":
                m._cancel.set()
                raise Cancelled("generated cancellation")
            if outcome == "fail":
                raise OSError("generated staging failure")
            m.install_repairs["missing.txt"] = "alternative/file.txt"
    monkeypatch.setattr(m.library.owned, "_checkpoint", fault)
    m.install("Repaired fixture")
    assert m._future.exception(timeout=10)
    m.poll()
    assert {p: p.read_bytes() for p in original} == original
    assert not any((m.library.root / "mods").iterdir())
    assert not any((m.library.root / ".staging").iterdir())
