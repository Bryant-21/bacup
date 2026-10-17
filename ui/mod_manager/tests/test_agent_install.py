import pytest

from creation_lib.mod_manager.agent_launch import create_instance, install
from creation_lib.mod_manager.paths import ManagerError
from .fixtures import make_package
from .test_standalone import game_fixture
from .test_reference_overlay import fixture as overlay_fixture  # noqa: F401


def test_cli_install_preview_choices_commit_and_stale_source_token(tmp_path):
    root = tmp_path / "library"
    create_instance(root, "fo4", str(game_fixture(tmp_path)), "Default")
    package = make_package(tmp_path)
    review = install(root, "Default", str(package))
    assert not review["errors"] and review["steps"] and review["plan"] and not review["applied"]
    assert not list((root / "mods").iterdir())
    result = install(root, "Default", str(package), apply=True, name="CLI fixture", expect=review["review_token"])
    destination = root / "mods" / result["mod_id"]
    assert result["applied"]
    assert {p.relative_to(destination).as_posix() for p in destination.rglob("*") if p.is_file()} == set(result["plan"])
    with pytest.raises(ManagerError, match="plan changed"):
        install(root, "Default", str(package), apply=True, name="Stale", expect="invalid-token")
    assert len(list((root / "mods").iterdir())) == 1


def test_plain_install_and_reinstall_use_the_same_owned_transaction(tmp_path):
    root = tmp_path / "library"
    create_instance(root, "fo4", str(game_fixture(tmp_path)), "Default")
    source = tmp_path / "plain"
    source.mkdir()
    (source / "fixture.txt").write_text("first")
    result = install(root, "Default", str(source), name="Plain", apply=True)
    (source / "fixture.txt").write_text("replacement")
    replacement = install(root, "Default", str(source), replace=result["mod_id"], apply=True)
    assert replacement["mod_id"] == result["mod_id"]
    assert (root / "mods" / result["mod_id"] / "fixture.txt").read_text() == "replacement"
    preview = install(root, "Default", str(source), replace=result["mod_id"])
    (root / "mods" / result["mod_id"] / "fixture.txt").write_text("user changes after review")
    with pytest.raises(ManagerError, match="plan changed"):
        install(root, "Default", str(source), replace=result["mod_id"], apply=True, expect=preview["review_token"])
    assert (root / "mods" / result["mod_id"] / "fixture.txt").read_text() == "user changes after review"


def test_cli_manual_mapping_uses_shared_review_and_disabled_install(tmp_path):
    root = tmp_path / "library"
    create_instance(root, "fo4", str(game_fixture(tmp_path)), "Default")
    source = tmp_path / "plain"
    source.mkdir()
    (source / "fixture.txt").write_text("payload")
    settings = {"mapping": {"fixture.txt": "config/renamed.txt"}}
    result = install(root, "Default", str(source), manual=True, adjustments=settings, disabled=True, apply=True)
    assert (root / "mods" / result["mod_id"] / "config/renamed.txt").read_text() == "payload"
    from creation_lib.mod_manager.library import Library
    library = Library(root)
    assert not library.rows(library.profiles()[0].id)[0].enabled


def test_cli_repairs_and_schema_policy_share_installer_validation(tmp_path):
    from .test_installer_repairs import repair_package
    root = tmp_path / "library"
    create_instance(root, "fo4", str(game_fixture(tmp_path)), "Default")
    package = repair_package(tmp_path / "repair")
    adjustments = {"repairs": {"missing.txt": "replacement.txt", "missing-folder": "alternative"}}
    result = install(root, "Default", str(package), policy="schema", adjustments=adjustments, apply=True)
    assert result["applied"] and set(result["plan"]) == {"textures/missing.txt", "meshes/file.txt"}


def test_cli_additive_update_preserves_reference(overlay_fixture):  # noqa: F811
    library, profile, row, incoming, _ = overlay_fixture
    preview = install(library.root, profile, str(incoming), overlay=row.id)
    result = install(library.root, profile, str(incoming), overlay=row.id, name="CLI update", expect=preview["review_token"], apply=True)
    rows = library.rows(profile)
    assert next(r for r in rows if r.id == result["mod_id"]).priority == next(r for r in rows if r.id == row.id).priority + 1
    assert (library.root / "mods" / result["mod_id"] / "shared.txt").read_bytes() == b"reviewed update"
