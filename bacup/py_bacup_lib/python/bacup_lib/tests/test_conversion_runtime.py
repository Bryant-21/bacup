import os
import pytest

from bacup_lib import conversion_runtime


def test_conversion_runtime_rejects_missing_or_stale_companion(tmp_path):
    project = tmp_path / "mods/B21_ConversionRuntime"
    project.joinpath("src").mkdir(parents=True)
    source = project / "src/main.cpp"
    source.write_text("runtime source", encoding="utf-8")
    with pytest.raises(FileNotFoundError, match="requires a built"):
        conversion_runtime.built_runtime_binary(tmp_path)
    binary = project / conversion_runtime.RUNTIME_DESTINATION
    binary.parent.mkdir(parents=True)
    binary.write_bytes(b"runtime DLL")
    os.utime(binary, ns=(1, 1))
    with pytest.raises(RuntimeError, match="predates its source"):
        conversion_runtime.built_runtime_binary(tmp_path)
    os.utime(binary, ns=(source.stat().st_mtime_ns + 1, source.stat().st_mtime_ns + 1))
    assert conversion_runtime.built_runtime_binary(tmp_path) == binary


def test_conversion_runtime_stages_beside_loose_catalog_without_game_write(tmp_path, monkeypatch):
    source = tmp_path / "compiled.dll"
    source.write_bytes(b"compiled runtime")
    monkeypatch.setattr(conversion_runtime, "runtime_binary", lambda: source)
    mod = tmp_path / "ConvertedSkyrim"
    catalog = mod / "F4SE/Plugins/B21_ConversionRuntime/Magic/Skyrim.esm.json"
    catalog.parent.mkdir(parents=True)
    catalog.write_text('{"version":1}', encoding="utf-8")
    destination = conversion_runtime.stage_conversion_runtime(mod)
    assert destination == mod / conversion_runtime.RUNTIME_DESTINATION
    assert destination.read_bytes() == source.read_bytes()
    assert catalog.read_text(encoding="utf-8") == '{"version":1}'


@pytest.mark.parametrize("relative", [
    "FalloutNV.creature-weapons.json", "FnvRadiation/FalloutNV.esm.json", "FnvImageSpace/FalloutNV.esm.json", "PropertySequences/FalloutNV.esm.json", "Weapons/FalloutNV.esm.json", "LegacyWeaponAmmo/FalloutNV.esm.json", "LegacyStats/FalloutNV.esm.json",
])
def test_fnv_native_catalog_stages_runtime_without_script_consumers(tmp_path, monkeypatch, relative):
    source = tmp_path / "compiled.dll"
    source.write_bytes(b"runtime")
    monkeypatch.setattr(conversion_runtime, "runtime_binary", lambda: source)
    mod = tmp_path / "ConvertedFNV"
    catalog = mod / "F4SE/Plugins/B21_ConversionRuntime" / relative
    catalog.parent.mkdir(parents=True)
    catalog.write_text('{"version":1}', encoding="utf-8")
    result = conversion_runtime.stage_fnv_catalog_runtime(mod, "FalloutNV.esm")
    assert result == mod / conversion_runtime.RUNTIME_DESTINATION
    assert result.read_bytes() == b"runtime"


def test_fnv_catalog_runtime_ignores_other_output_plugins(tmp_path, monkeypatch):
    monkeypatch.setattr(conversion_runtime, "runtime_binary", lambda: pytest.fail("unused runtime requested"))
    unrelated = tmp_path / "F4SE/Plugins/B21_ConversionRuntime/Other.creature-weapons.json"
    unrelated.parent.mkdir(parents=True)
    unrelated.write_text('{"version":1}', encoding="utf-8")
    assert conversion_runtime.stage_fnv_catalog_runtime(tmp_path, "FalloutNV.esm") is None


def test_skyrim_magic_phase_retains_mapper_until_catalog_and_companion_are_ready(tmp_path, monkeypatch):
    from pathlib import Path
    from types import SimpleNamespace
    from bacup_lib.tests.test_unified_driver import (
        StubRunner, UnifiedDriver, make_request, stub_record_runtime,
    )

    request = make_request(tmp_path)
    request.source_game = "skyrimse"
    request.options.convert_scripts = False
    driver = UnifiedDriver(request, sink_id=None)
    calls = []
    stub_record_runtime(driver, [], monkeypatch)
    original_context = driver.record_runtime._build_context

    class FakeRun:
        id = 123
        mapper_live = True
        source_live = True

        def run_phase(self, phase, **kwargs):
            assert phase == "prepare_skyrim_magic_runtime"
            assert self.mapper_live and self.source_live
            calls.append("prepare")

        def release_remap_state(self):
            self.mapper_live = False
            calls.append("mapper release")

        def release_master_handles(self):
            calls.append("masters release")
            return 0

        def release_source_handle(self):
            self.source_live = False
            calls.append("source release")
            return False

        def save_target(self, path, **kwargs):
            assert not self.source_live
            calls.append("save")
            Path(path).write_bytes(b"final plugin")

    def make_context(*args, **kwargs):
        context = original_context(*args, **kwargs)
        context._rust_conversion_run = FakeRun()
        return context

    def execute_phase(number, label, body, runner, **kwargs):
        if label in {"Skyrim Creature Magic Runtime", "Build ESP"}:
            body(SimpleNamespace())

    monkeypatch.setattr(driver.record_runtime, "_build_context", make_context)
    monkeypatch.setattr(driver.record_runtime, "_run_phase", execute_phase)
    monkeypatch.setattr(conversion_runtime, "stage_conversion_runtime", lambda path: calls.append("stage DLL"))
    driver.run_record_track(StubRunner())
    assert calls == ["prepare", "stage DLL", "source release", "save"]


def clock_consumer(tmp_path, source):
    path = tmp_path / "Scripts/Source/User/RavenProbe.psc"
    path.parent.mkdir(parents=True)
    path.write_text(source, encoding="utf-8")
    return [{
        "class_name": "RavenProbe",
        "relative_source_path": "Scripts/Source/User/RavenProbe.psc",
        "kind": "standalone",
        "required": True,
    }]


def test_fnv_native_helpers_and_consumer_compile_and_package(tmp_path, monkeypatch):
    from types import SimpleNamespace
    from bacup_lib.tests.test_unified_driver import StubRunner, UnifiedDriver, make_request

    manifest = clock_consumer(tmp_path, """Scriptname RavenProbe
Float Function Clock()
Return B21_ConversionClock.GetGameModeClock()
EndFunction
Bool Function GameplayActive()
Return B21_ConversionClock.CanRunGameMode()
EndFunction
Bool Function Limb(Actor target)
Return B21_ConversionCreature.IsLimbGone(target, 2)
EndFunction
Function KillWithLimb(Actor target, Actor killer)
B21_ConversionCreature.KillActorWithLimb(target, killer, 0)
EndFunction
Function RequestFrame()
B21_ConversionClock.RequestNextGameMode(Self, 9)
B21_ConversionClock.CancelGameMode(Self)
EndFunction
""")
    request = make_request(tmp_path)
    request.source_game = "fnv"
    request.options.convert_scripts = True
    driver = UnifiedDriver(request, sink_id=None)
    headers = tmp_path / "headers"
    headers.mkdir()
    (headers / "Actor.psc").write_text("Scriptname Actor Hidden\n", encoding="utf-8")
    monkeypatch.setattr(driver.record_runtime, "_papyrus_import_roots", lambda *args: (headers,))
    runtime = tmp_path / "runtime.dll"
    runtime.write_bytes(b"native clock runtime")
    monkeypatch.setattr(conversion_runtime, "runtime_binary", lambda: runtime)
    evidence = driver.record_runtime._compile_fnv_generated_psc_manifest(
        manifest=manifest,
        psc_files_written=1,
        ctx=SimpleNamespace(mod_path=tmp_path),
        runner=StubRunner(),
    )
    assert {entry["class_name"] for entry in evidence if entry["compiled_success"]} == {
        "RavenProbe", conversion_runtime.CLOCK_CLASS, "B21_ConversionCreature",
    }
    for entry in evidence:
        assert (tmp_path / entry["relative_pex_path"]).stat().st_size > 0
    assert (tmp_path / conversion_runtime.RUNTIME_DESTINATION).read_bytes() == runtime.read_bytes()
    helper = tmp_path / "Scripts/Source/User/B21_ConversionClock.psc"
    assert "GetGameModeClock() Global Native" in helper.read_text()
    creature = tmp_path / "Scripts/Source/User/B21_ConversionCreature.psc"
    assert "IsLimbGone(Actor akActor, Int aiSourcePart) Global Native" in creature.read_text()
    assert "KillActorWithLimb(Actor akActor, Actor akKiller, Int aiSourcePart) Global Native" in creature.read_text()


@pytest.mark.parametrize("function", ["RequestNextGameMode", "CancelGameMode", "CanRunGameMode"])
def test_fnv_cadence_only_consumer_requires_clock(tmp_path, function):
    manifest = clock_consumer(tmp_path, f"Scriptname RavenProbe\nB21_ConversionClock.{function}(Self)\n")
    result, consumers = conversion_runtime.prepare_fnv_native_manifest(tmp_path, manifest)
    assert consumers == {conversion_runtime.CLOCK_CLASS: {"RavenProbe"}}
    assert len(result) == 2


@pytest.mark.parametrize("function", ["IsLimbGone", "KillActorWithLimb"])
def test_fnv_creature_native_demand_does_not_add_unused_clock(tmp_path, function):
    manifest = clock_consumer(tmp_path, f"Scriptname RavenProbe\nB21_ConversionCreature.{function}(target, 0)\n")
    result, consumers = conversion_runtime.prepare_fnv_native_manifest(tmp_path, manifest)
    assert consumers == {"B21_ConversionCreature": {"RavenProbe"}}
    assert {entry["class_name"] for entry in result} == {"RavenProbe", "B21_ConversionCreature"}
    assert not (tmp_path / "Scripts/Source/User/B21_ConversionClock.psc").exists()


def test_fnv_runtime_is_not_required_without_a_compiled_clock_consumer(tmp_path, monkeypatch):
    manifest = clock_consumer(tmp_path, "Scriptname RavenProbe\n; B21_ConversionClock.GetGameModeClock()\n")
    returned, consumers = conversion_runtime.prepare_fnv_native_manifest(tmp_path, manifest)
    assert returned == manifest and not consumers
    assert not (tmp_path / "Scripts/Source/User/B21_ConversionClock.psc").exists()
    monkeypatch.setattr(conversion_runtime, "runtime_binary", lambda: pytest.fail("unused runtime requested"))
    assert conversion_runtime.stage_fnv_native_runtime(tmp_path, {conversion_runtime.CLOCK_CLASS: {"RavenProbe"}}, [{
        "class_name": "RavenProbe", "compiled_success": False,
    }]) is None
    assert not (tmp_path / conversion_runtime.RUNTIME_DESTINATION).exists()


def test_fnv_clock_requires_fresh_helper_pex_before_packaging(tmp_path):
    evidence = [{"class_name": "RavenProbe", "compiled_success": True}]
    with pytest.raises(RuntimeError, match="without required"):
        conversion_runtime.stage_fnv_native_runtime(tmp_path, {conversion_runtime.CLOCK_CLASS: {"RavenProbe"}}, evidence)
    evidence.append({
        "class_name": conversion_runtime.CLOCK_CLASS,
        "compiled_success": True,
        "relative_pex_path": "data/Scripts/B21_ConversionClock.pex",
    })
    with pytest.raises(RuntimeError, match="missing or empty"):
        conversion_runtime.stage_fnv_native_runtime(tmp_path, {conversion_runtime.CLOCK_CLASS: {"RavenProbe"}}, evidence)
