import threading

import pytest

from creation_lib.mod_manager.controller import ManagerController
from creation_lib.mod_manager.library import Library
from .test_controller import finish


def make_policy_package(directory):
    directory.mkdir()
    (directory / "fomod").mkdir()
    for name in ("base", "first", "second"):
        (directory / (name + ".txt")).write_bytes(name.encode())
    options = ''.join(f'<plugin name="{name}"><conditionFlags><flag name="choice">{value}</flag></conditionFlags><typeDescriptor><type name="Required"/></typeDescriptor></plugin>' for name, value in (("A", "first"), ("B", "second")))
    patterns = ''.join(f'<pattern><dependencies><flagDependency flag="choice" value="{value}"/></dependencies><files><file source="{value}.txt" destination="chosen.txt"/></files></pattern>' for value in ("first", "second"))
    xml = f'<config><moduleName>Policy fixture</moduleName><requiredInstallFiles><file source="base.txt"/></requiredInstallFiles><installSteps><installStep name="Flags"><optionalFileGroups><group name="Both" type="SelectAny"><plugins order="Explicit">{options}</plugins></group></optionalFileGroups></installStep></installSteps><conditionalFileInstalls><patterns>{patterns}</patterns></conditionalFileInstalls></config>'
    (directory / "fomod" / "ModuleConfig.xml").write_text(xml, encoding="utf-8")
    return directory


@pytest.fixture
def manager(tmp_path):
    m = ManagerController()
    m.library = Library(tmp_path / "library", create=True)
    m.profile_id = m.library.create_profile("Main")
    m.refresh()
    m.prepare_install(make_policy_package(tmp_path / "policy package"))
    finish(m)
    yield m
    m.cleanup()


def test_new_install_defaults_to_mo2_and_policy_change_requires_new_review(manager):
    m = manager
    assert m.prepared.fomod.compatibility == "mo2"
    m.review_install()
    finish(m)
    assert m.install_plan == {"base.txt": "base.txt", "chosen.txt": "first.txt"}
    m.install_mapping = {"base.txt": "relocated.txt"}
    m.install_additions = {"extra.txt": "first.txt"}
    before = m.prepared
    m.change_fomod_policy("schema")
    finish(m)
    assert m.prepared is not before and m.prepared.context is before.context
    assert m.install_plan is None and not m.install_mapping and not m.install_additions and not m.choices
    m.review_install()
    finish(m)
    assert m.install_plan == {"base.txt": "base.txt", "chosen.txt": "second.txt"}
    m.install("Policy result")
    finish(m)
    assert (m.library.root / "mods" / m.selected_id / "chosen.txt").read_bytes() == b"second"


@pytest.mark.parametrize("outcome", ["cancelled", "failed"])
def test_policy_reparse_cancel_failure_keeps_prior_review_and_choices(manager, monkeypatch, outcome):
    m = manager
    m.review_install()
    finish(m)
    m.choices = {"0:0": {"0:0:0", "0:0:1"}}
    before, plan, choices = m.prepared, dict(m.install_plan), dict(m.choices)
    entered, release = threading.Event(), threading.Event()
    def stop():
        if outcome == "failed":
            raise OSError("disposable policy parser failure")
        entered.set()
        assert release.wait(10)
    monkeypatch.setattr("creation_lib.mod_manager.fomod_compatibility._checkpoint", stop)
    m.change_fomod_policy("schema")
    try:
        if outcome == "cancelled":
            assert entered.wait(10)
            m.cancel()
    finally:
        release.set()
    m._future.exception(timeout=10)
    m.poll()
    assert m.job_state == outcome and m.prepared is before
    assert m.install_plan == plan and m.choices == choices and not m.library.rows(m.profile_id)
