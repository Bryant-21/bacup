from pathlib import Path
from types import SimpleNamespace

import pytest

from bacup_lib.worldspace_services import patch_target_worldspaces_subrecords
from creation_lib.esp.plugin import replace_plugin_with_localized_sidecars


@pytest.mark.parametrize(
    ("temp_name", "temp_sidecar_stem"),
    [
        ("B21_Test.esm.tmp", "B21_Test.esm"),
        ("B21_Test.esm.wrldcarry.tmp", "B21_Test.esm.wrldcarry"),
    ],
)
def test_replace_plugin_with_localized_sidecars_uses_final_plugin_stem(
    tmp_path,
    temp_name: str,
    temp_sidecar_stem: str,
) -> None:
    plugin_path = tmp_path / "B21_Test.esm"
    temp_plugin_path = tmp_path / temp_name
    strings_dir = tmp_path / "Strings"
    strings_dir.mkdir()
    plugin_path.write_bytes(b"old")
    temp_plugin_path.write_bytes(b"new")
    (strings_dir / "B21_Test_en.STRINGS").write_bytes(b"old strings")
    (strings_dir / f"{temp_sidecar_stem}_en.STRINGS").write_bytes(b"new strings")
    (strings_dir / f"{temp_sidecar_stem}_en.DLSTRINGS").write_bytes(b"new dlstrings")

    replace_plugin_with_localized_sidecars(temp_plugin_path, plugin_path)

    assert plugin_path.read_bytes() == b"new"
    assert (strings_dir / "B21_Test_en.STRINGS").read_bytes() == b"new strings"
    assert (strings_dir / "B21_Test_en.DLSTRINGS").read_bytes() == b"new dlstrings"
    assert not temp_plugin_path.exists()
    assert not (strings_dir / f"{temp_sidecar_stem}_en.STRINGS").exists()
    assert not (strings_dir / f"{temp_sidecar_stem}_en.DLSTRINGS").exists()


def test_worldspace_batch_loads_and_saves_target_once(tmp_path, monkeypatch) -> None:
    from creation_lib.esp import Plugin
    from creation_lib.esp import plugin as plugin_module

    events: list[object] = []

    class TargetPlugin:
        def carry_worldspace_header_from_source(
            self,
            _source_plugin,
            *,
            source_worldspace_editor_id,
            target_worldspace_editor_id,
        ):
            assert source_worldspace_editor_id == target_worldspace_editor_id
            events.append(("carry", source_worldspace_editor_id))
            return {"copied": 3, "warnings": []}

        def sync_cell_max_height_from_source(
            self,
            _source_plugin,
            *,
            source_worldspace_editor_id,
            target_worldspace_editor_id,
        ):
            assert source_worldspace_editor_id == target_worldspace_editor_id
            events.append(("mhdt", source_worldspace_editor_id))
            return {"cells_changed": 1, "warnings": []}

        def save(self, path):
            events.append(("save", Path(path)))

        def close(self):
            events.append("close")

    target = TargetPlugin()
    target_path = tmp_path / "Starfield.esm"

    def fake_load(path, *, game):
        events.append(("load", Path(path), game))
        return target

    monkeypatch.setattr(Plugin, "load", staticmethod(fake_load))
    monkeypatch.setattr(
        plugin_module,
        "replace_plugin_with_localized_sidecars",
        lambda source, destination: events.append(
            ("replace", Path(source), Path(destination))
        ),
    )

    copied = patch_target_worldspaces_subrecords(
        source_plugin=SimpleNamespace(is_localized=False),
        target_plugin_path=target_path,
        worldspace_editor_ids=["WorldA", "WorldB"],
        target_game="fo4",
    )

    assert copied == [3, 3]
    assert [
        event for event in events if isinstance(event, tuple) and event[0] == "load"
    ] == [("load", target_path, "fo4")]
    assert [
        event for event in events if isinstance(event, tuple) and event[0] == "save"
    ] == [("save", Path(f"{target_path}.wrldcarry.tmp"))]
    assert events.count("close") == 1
    assert [
        event for event in events if isinstance(event, tuple) and event[0] == "replace"
    ] == [
        (
            "replace",
            Path(f"{target_path}.wrldcarry.tmp"),
            target_path,
        )
    ]
    assert [("carry", "WorldA"), ("carry", "WorldB")] == [
        event for event in events if isinstance(event, tuple) and event[0] == "carry"
    ]
