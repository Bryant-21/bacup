"""Integration tests for the path-owned ConversionRun boundary."""
from __future__ import annotations

import gc
import shutil
from pathlib import Path

import pytest


def _find_fixture() -> Path:
    here = Path(__file__).resolve()
    for parent in [here, *here.parents]:
        candidate = (
            parent
            / "bacup"
            / "py_bacup_lib"
            / "native"
            / "conversion"
            / "src"
            / "test_fixtures"
            / "fo4_minimal_weap.esm"
        )
        if candidate.exists():
            return candidate
    return here.parents[5] / "bacup/py_bacup_lib/native/conversion/src/test_fixtures/fo4_minimal_weap.esm"


FIXTURE = _find_fixture()


def _native():
    from bacup_lib.native_runtime import load_native_module

    return load_native_module()


class TestPathBoundary:
    def test_raw_handle_constructor_is_not_exported(self):
        assert not hasattr(_native()._raw, "conversion_run_create")
        assert not hasattr(_native()._raw, "conversion_run_source_handle")
        assert not hasattr(_native()._raw, "conversion_run_target_handle")

    @pytest.mark.parametrize(
        ("target_name", "target_path"),
        [(None, None), ("Output.esm", "Output.esm")],
    )
    def test_exactly_one_target_mode_is_required(self, target_name, target_path):
        with pytest.raises(ValueError, match="exactly one"):
            _native().conversion_run_create_from_paths(
                "fo4", "fo4", None, target_name, target_path, [], None, {}
            )

    def test_create_new_owns_target_and_closes_idempotently(self, tmp_path):
        from bacup_lib.run import ConversionRun

        run = ConversionRun.create_new(
            "fo4",
            "fo4",
            None,
            "Output.esm",
            config={"mod_path": str(tmp_path)},
        )
        assert not hasattr(run, "_source_handle_id")
        assert not hasattr(run, "_target_handle_id")
        assert run.release_source_handle() is False
        assert run.release_master_handles() == 0
        run.close()
        run.close()

    def test_context_manager_drops_on_exception(self, tmp_path):
        from bacup_lib.run import ConversionRun

        run_id = None
        with pytest.raises(ValueError, match="intentional"):
            with ConversionRun.create_new(
                "fo4", "fo4", None, "Output.esm", config={"mod_path": str(tmp_path)}
            ) as run:
                run_id = run.id
                raise ValueError("intentional")
        with pytest.raises(Exception):
            _native().conversion_run_drain_decisions(run_id)

    def test_del_best_effort_drops_run(self, tmp_path):
        from bacup_lib.run import ConversionRun

        run = ConversionRun.create_new(
            "fo4", "fo4", None, "Output.esm", config={"mod_path": str(tmp_path)}
        )
        run_id = run.id
        del run
        gc.collect()
        with pytest.raises(Exception):
            _native().conversion_run_drain_decisions(run_id)

    def test_save_uses_default_and_override_paths(self, tmp_path):
        from bacup_lib.run import ConversionRun

        override = tmp_path / "override.esm"
        with ConversionRun.create_new(
            "fo4", "fo4", None, "Output.esm", config={"mod_path": str(tmp_path)}
        ) as run:
            run.save_target(run_nvnm_validator=False)
            assert (tmp_path / "Output.esm").is_file()
            run.save_target(str(override), run_nvnm_validator=False)
            assert override.is_file()

    def test_source_dependent_phase_fails_clearly(self, tmp_path):
        from bacup_lib.run import ConversionRun

        with ConversionRun.create_new(
            "fo4", "fo4", None, "Output.esm", config={"mod_path": str(tmp_path)}
        ) as run:
            with pytest.raises(RuntimeError, match="requires a source plugin"):
                _native().conversion_run_translate_all(run.id)

    @pytest.mark.parametrize("phase", ["translate_v2", "walk", "convert_terrain"])
    def test_source_dependent_dispatcher_phases_fail_clearly(self, tmp_path, phase):
        from bacup_lib.run import ConversionRun

        with ConversionRun.create_new(
            "fo4", "fo4", None, "Output.esm", config={"mod_path": str(tmp_path)}
        ) as run:
            with pytest.raises(
                RuntimeError, match="this phase requires a source plugin"
            ):
                run.run_phase(phase, mod_path=str(tmp_path), params={})

    def test_source_free_dispatcher_phase_runs_without_source(self, tmp_path):
        from bacup_lib.run import ConversionRun

        with ConversionRun.create_new(
            "fo4", "fo4", None, "Output.esm", config={"mod_path": str(tmp_path)}
        ) as run:
            report = run.run_phase(
                "record_translation_maps", mod_path=str(tmp_path), params={}
            )
        assert report["warnings"] == 0

    @pytest.mark.parametrize(
        ("source_game", "phase", "params", "legacy_key"),
        [
            ("fo4", "walk", {"source_handle": 1}, "source_handle"),
            ("fo4", "walk", {"master_handles": [1]}, "master_handles"),
            ("fo4", "graft_terrain", {"prior_handle_id": 1}, "prior_handle_id"),
            ("fo4", "regenerate_modt", {"output_handle_id": 1}, "output_handle_id"),
            (
                "fo4",
                "regenerate_modt",
                {"deployed_esm_handle_id": 1},
                "deployed_esm_handle_id",
            ),
            ("fo76", "convert_terrain", {"source_handle_id": 1}, "source_handle_id"),
            ("fo76", "convert_terrain", {"target_handle_id": 1}, "target_handle_id"),
            (
                "fo76",
                "convert_terrain",
                {"record_output_mode": "target_handle"},
                "record_output_mode",
            ),
        ],
    )
    def test_dispatcher_rejects_legacy_handle_overrides(
        self, tmp_path, source_game, phase, params, legacy_key
    ):
        from bacup_lib.run import ConversionRun
        from creation_lib.esp import Plugin

        source_path = tmp_path / "Source.esm"
        source = Plugin.new(source_path.name, game=source_game)
        try:
            source.save(source_path)
        finally:
            source.close()

        with ConversionRun.create_new(
            source_game,
            "fo4",
            str(source_path),
            "Output.esm",
            config={"mod_path": str(tmp_path)},
        ) as run:
            with pytest.raises((ValueError, RuntimeError), match=legacy_key):
                run.run_phase(phase, mod_path=str(tmp_path), params=params)

    def test_lod_collection_rejects_missing_source_and_dropped_run(self, tmp_path):
        from bacup_lib.run import ConversionRun

        run = ConversionRun.create_new(
            "fo4", "fo4", None, "Output.esm", config={"mod_path": str(tmp_path)}
        )
        with pytest.raises(RuntimeError, match="source plugin"):
            run.collect_lod_closures()
        run.close()
        with pytest.raises(RuntimeError, match="unknown conversion run"):
            run.collect_lod_closures()

    def test_script_reference_collection_uses_run_target_registry(self, tmp_path):
        from bacup_lib.run import ConversionRun

        with ConversionRun.create_new(
            "fo4", "fo4", None, "Output.esm", config={"mod_path": str(tmp_path)}
        ) as run:
            plugin_name, rows = _native().conversion_run_script_reference_records(
                run.id, ["VMAD", "CTDA"]
            )

        assert plugin_name == "Output.esm"
        assert rows == []


@pytest.mark.skipif(not FIXTURE.exists(), reason=f"fixture not present: {FIXTURE}")
class TestPathBoundaryWithPlugin:
    def test_translate_all_uses_conversion_local_handles(self, tmp_path):
        from bacup_lib.run import ConversionRun

        with ConversionRun.create_new(
            "fo4",
            "fo4",
            str(FIXTURE),
            "Translated.esm",
            config={"mod_path": str(tmp_path)},
        ) as run:
            stats = _native().conversion_run_translate_all(run.id)
            assert stats["records_translated"] >= 1
            run.save_target(run_nvnm_validator=False)
        assert (tmp_path / "Translated.esm").is_file()

    def test_create_new_and_early_source_release(self, tmp_path):
        from bacup_lib.run import ConversionRun

        with ConversionRun.create_new(
            "fo4",
            "fo4",
            str(FIXTURE),
            "Output.esm",
            config={"mod_path": str(tmp_path)},
        ) as run:
            assert run.release_source_handle() is True
            assert run.release_source_handle() is False
            for phase in ("translate_v2", "walk", "convert_terrain"):
                with pytest.raises(
                    RuntimeError, match="this phase requires a source plugin"
                ):
                    run.run_phase(phase, mod_path=str(tmp_path), params={})

    def test_open_existing_defaults_save_to_original_path(self, tmp_path):
        from bacup_lib.run import ConversionRun

        target = tmp_path / "Existing.esm"
        with ConversionRun.create_new(
            "fo4", "fo4", None, target.name, config={"mod_path": str(tmp_path)}
        ) as run:
            run.save_target(run_nvnm_validator=False)
        before = target.stat().st_size
        with ConversionRun.open_existing("fo4", "fo4", None, str(target)) as run:
            run.save_target(run_nvnm_validator=False)
        assert target.is_file()
        assert target.stat().st_size == before

    def test_open_existing_preserves_target_header_masters_strings_and_next_id(
        self, tmp_path
    ):
        from bacup_lib.run import ConversionRun
        from creation_lib.esp.plugin import Plugin

        target_path = tmp_path / "Existing.esm"
        lookup_master = tmp_path / "Lookup.esm"
        shutil.copyfile(FIXTURE, lookup_master)

        target = Plugin.new(target_path.name, game="fo4")
        try:
            target.add_master("Original.esm", size=123)
            target.header.author = "Keep Author"
            target.header.description = "Keep Description"
            target.header.flags = 0x80
            target.header.next_object_id = 0x1234
            target.set_localized_strings_by_language(
                {"en": {0x445566: "Keep localized text"}},
                preferred_language="en",
                table_types={0x445566: "strings"},
            )
            target.save(target_path)
        finally:
            target.close()

        with ConversionRun.open_existing(
            "fo4",
            "fo4",
            str(FIXTURE),
            str(target_path),
            master_plugin_paths=[str(lookup_master)],
            source_strings_dir=str(tmp_path / "Strings"),
            config={"generated_object_id_floor": 0xF000},
        ) as run:
            run.save_target(run_nvnm_validator=False)

        preserved = Plugin.load(
            target_path,
            game="fo4",
            strings_dir=tmp_path / "Strings",
        )
        try:
            assert preserved.header.masters == ["Original.esm"]
            assert preserved.header.author == "Keep Author"
            assert preserved.header.description == "Keep Description"
            assert preserved.header.flags == 0x80
            assert preserved.header.next_object_id == 0x1234
            assert preserved.localized_strings_by_language == {
                "en": {0x445566: "Keep localized text"}
            }
        finally:
            preserved.close()

    def test_generated_id_floor_survives_save(self, tmp_path):
        from bacup_lib.run import ConversionRun

        target = tmp_path / "Generated.esm"
        with ConversionRun.create_new(
            "fo4",
            "fo4",
            str(FIXTURE),
            target.name,
            config={"mod_path": str(tmp_path), "generated_object_id_floor": 0x9000},
        ) as run:
            run.save_target(run_nvnm_validator=False)
        assert target.is_file()

    def test_master_paths_preserve_caller_order(self, tmp_path):
        from bacup_lib.run import ConversionRun
        from creation_lib.esp.plugin import Plugin

        first = tmp_path / "First.esm"
        second = tmp_path / "Second.esm"
        shutil.copyfile(FIXTURE, first)
        shutil.copyfile(FIXTURE, second)
        with ConversionRun.create_new(
            "fo4",
            "fo4",
            str(FIXTURE),
            "Ordered.esm",
            master_plugin_paths=[str(first), str(second)],
            config={"mod_path": str(tmp_path)},
        ) as run:
            run.save_target(run_nvnm_validator=False)
            assert run.release_master_handles() == 2
            assert run.release_master_handles() == 0
        output = Plugin.load(tmp_path / "Ordered.esm", game="fo4")
        try:
            assert output.header.masters == ["First.esm", "Second.esm"]
        finally:
            output.close()
