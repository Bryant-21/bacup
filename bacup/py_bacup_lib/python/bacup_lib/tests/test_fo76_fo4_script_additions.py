from __future__ import annotations

import tomllib
from pathlib import Path

import pytest

from bacup_lib.workflows import unified
from bacup_lib.tests.native_script_fixtures import (
    assert_pex,
    native_scripts,
    script_path,
    write_pex,
)


TRIGGER_SCRIPT = "B21:StoryEventOnTriggerEnter"
ACTIVATION_SCRIPT = "B21:StoryEventOnActivateStartScene"
REWARD_SCRIPT = "B21:QuestRewards"
MATERIALIZER_SCRIPT = "B21:LocalEncounterMaterializer"
TWO_STATE_ADAPTER_SCRIPT = "B21TwoStateActivator76"
MUSIC_INSTRUMENT_SCRIPT = "B21MusicInstrumentScript"
FURNITURE_BUFF_SCRIPT = "B21:FurnitureBuff"
HOLOTAPE_STAGE_SCRIPT = "B21:HolotapeStageOnPlay"
COLLECTOR_SCRIPT = "B21:WorkshopCollector"
MARCIA_NOTE_SCRIPT = "Fragments:Packages:PF_BS02_MQ02_Missing_Marci_0060E191_2"
MANIFEST_NAMES = tuple(
    sorted(
        unified.load_native_module().script_addition_names("fo76", "fo4"), key=str.lower
    )
)
# The subset the VMAD-reference fixtures below drive directly; additions the
# conversion injects from a consumer script instead are in MANIFEST_NAMES only.
SCRIPT_NAMES = tuple(
    sorted(
        (
            TRIGGER_SCRIPT,
            ACTIVATION_SCRIPT,
            REWARD_SCRIPT,
            "B21:CurrencyQuestRewards",
            "B21:ExpeditionMissionRewards",
            MATERIALIZER_SCRIPT,
            "B21:EnclaveEventSupport",
            HOLOTAPE_STAGE_SCRIPT,
            COLLECTOR_SCRIPT,
            TWO_STATE_ADAPTER_SCRIPT,
            MUSIC_INSTRUMENT_SCRIPT,
            FURNITURE_BUFF_SCRIPT,
            "B21_PlayerFear",
            "B21:PlanLearnOnRead",
            "B21_ActivateAliasWithRequiredItem",
            "B21_ShowMessageOnActivateAlias",
            "B21:KeypadNative",
            MARCIA_NOTE_SCRIPT,
        ),
        key=str.lower,
    )
)


def _relative_path(script_name: str, suffix: str = ".psc") -> Path:
    return Path(*script_name.split(":")).with_suffix(suffix)


def _addition_path(script_name: str) -> Path:
    return unified._SCRIPT_ADDITION_DIR / "fo76_fo4" / _relative_path(script_name)


def test_script_addition_discovery_is_pair_scoped_and_namespace_correct():
    additions = unified._script_addition_sources("FO76", "FO4")

    assert tuple(sorted(additions)) == tuple(
        sorted(unified._script_key(script_name) for script_name in MANIFEST_NAMES)
    )
    for script_name in MANIFEST_NAMES:
        key = unified._script_key(script_name)
        assert additions[key] == (script_name, _addition_path(script_name))
        assert additions[key][1].relative_to(
            unified._SCRIPT_ADDITION_DIR / "fo76_fo4"
        ) == _relative_path(script_name)
    assert unified._script_addition_sources("skyrimse", "fo4") == {}
    assert unified._script_addition_sources("fnvfo3", "fo4") == {}


def test_script_additions_are_included_in_wheel_package_data():
    pyproject_path = Path(__file__).resolve().parents[3] / "pyproject.toml"
    pyproject = tomllib.loads(pyproject_path.read_text(encoding="utf-8"))
    wheel_includes = {
        item["path"]
        for item in pyproject["tool"]["maturin"]["include"]
        if item.get("format") == "wheel"
    }

    assert "python/bacup_lib/script_additions/**/*.psc" in wheel_includes
    assert tuple(sorted(unified._script_addition_sources("fo76", "fo4"))) == tuple(
        sorted(unified._script_key(script_name) for script_name in MANIFEST_NAMES)
    )


def test_required_script_additions_are_exact_and_pair_scoped():
    for script_name in MANIFEST_NAMES:
        assert unified._requires_script_addition(
            script_name,
            source_game="FO76",
            target_game="FO4",
        )

    assert not unified._requires_script_addition(
        "B21:Unrelated",
        source_game="fo76",
        target_game="fo4",
    )
    assert not unified._requires_script_addition(
        TRIGGER_SCRIPT,
        source_game="skyrimse",
        target_game="fo4",
    )


def test_every_pair_scoped_addition_source_is_registered():
    # The manifest is the only delivery path, so an unlisted .psc never reaches
    # the mod: callers compile against nothing and VMAD binds a missing class.
    pair_root = unified._SCRIPT_ADDITION_DIR / "fo76_fo4"
    on_disk = {
        unified._script_key(":".join(path.relative_to(pair_root).with_suffix("").parts))
        for path in pair_root.rglob("*.psc")
    }

    assert sorted(on_disk - set(unified._script_addition_sources("fo76", "fo4"))) == []


@pytest.mark.parametrize("script_name", ("B21:Unrelated", "ObjectReference"))
def test_nonmanifest_script_additions_use_normal_target_resolution(
    script_name, tmp_path
):
    with native_scripts(tmp_path, [(script_name, "ACTI")]) as fixture:
        fixture.addition(script_name)
        target_pex = fixture.target / "Scripts" / script_path(script_name, ".pex")
        write_pex(target_pex, script_name)
        report = fixture.convert()
        assert report["scripts"][0]["status"] == "target"
        assert Path(report["scripts"][0]["pex_path"]) == target_pex
        assert not fixture.psc(script_name).exists()
        assert [ref[0] for ref in fixture.references()] == [script_name]


def test_script_addition_install_uses_namespace_path_and_validates_declaration(
    tmp_path: Path,
):
    source_path = tmp_path / "source" / "B21" / "TargetOnly.psc"
    source_path.parent.mkdir(parents=True)
    source_path.write_text(
        "Scriptname B21:TargetOnly extends ObjectReference\n",
        encoding="utf-8",
    )

    installed = unified._install_script_addition(
        tmp_path / "mod",
        "B21:TargetOnly",
        source_path,
    )

    assert installed == (
        tmp_path / "mod" / "Scripts" / "Source" / "User" / "B21" / "TargetOnly.psc"
    )
    assert installed.read_text(encoding="utf-8") == source_path.read_text(
        encoding="utf-8"
    )

    source_path.write_text("Scriptname B21:Wrong\n", encoding="utf-8")
    with pytest.raises(ValueError, match="declares B21:Wrong, expected B21:TargetOnly"):
        unified._install_script_addition(
            tmp_path / "mod",
            "B21:TargetOnly",
            source_path,
        )


def test_duplicate_references_install_compile_and_report_each_addition_once(tmp_path):
    signatures = (
        "ACTI",
        "QUST",
        "MGEF",
        "NOTE",
        "MSTT",
        "FURN",
        "CONT",
        "BOOK",
        "PACK",
    )
    refs = [
        (name, signatures[i % len(signatures)])
        for i, name in enumerate(SCRIPT_NAMES)
        for _ in range(2)
    ]
    with native_scripts(tmp_path, refs) as fixture:
        additions = {name: fixture.addition(name) for name in SCRIPT_NAMES}
        report = fixture.convert()
        assert [item["script_name"] for item in report["scripts"]] == list(SCRIPT_NAMES)
        assert all(item["status"] == "compiled" for item in report["scripts"])
        assert len(fixture.references()) == len(refs)
        events = fixture.run.drain_events(1000)
        for name in SCRIPT_NAMES:
            assert fixture.psc(name).read_text() == additions[name].read_text()
            assert_pex(fixture.pex(name), name)
            assert (
                sum(
                    f"installed target-only full script {name}"
                    in event.get("message", "")
                    for event in events
                )
                == 1
            )
        compile_event = next(
            event
            for event in fixture.ctx.timing_report.events
            if event["name"] == "convert_scripts_compile"
        )
        assert compile_event["candidates"] == len(SCRIPT_NAMES)


@pytest.mark.parametrize(
    ("consumer", "record_sig", "addition"),
    [
        ("Creatures:WendigoColossusRaceScript", "MGEF", "B21_PlayerFear"),
        ("ENz01_AboveScript", "QUST", "B21:EnclaveEventSupport"),
        ("W05_Wayward_SwapMarkerOnCriteria", "ACTI", "B21_WaywardState"),
        (
            "Fragments:Terminals:TERM_SCORE_Vending_Terminal__005D2AE0",
            "TERM",
            "B21:AmmoConverter",
        ),
    ],
)
def test_consumer_script_reference_installs_its_shared_addition(
    tmp_path, consumer, record_sig, addition
):
    with native_scripts(tmp_path, [(consumer, record_sig)]) as fixture:
        source = fixture.addition(addition)
        report = fixture.convert()
        assert (
            next(row for row in report["scripts"] if row["script_name"] == addition)[
                "status"
            ]
            == "compiled"
        )
        assert_pex(fixture.pex(addition), addition)
        assert fixture.psc(addition).read_text() == source.read_text()


def test_unreferenced_addition_is_not_installed(tmp_path):
    with native_scripts(tmp_path, [(TRIGGER_SCRIPT, "ACTI")]) as fixture:
        for name in (TRIGGER_SCRIPT, ACTIVATION_SCRIPT, REWARD_SCRIPT):
            fixture.addition(name)
        report = fixture.convert()
        assert [row["script_name"] for row in report["scripts"]] == [TRIGGER_SCRIPT]
        assert_pex(fixture.pex(TRIGGER_SCRIPT), TRIGGER_SCRIPT)
        for name in (ACTIVATION_SCRIPT, REWARD_SCRIPT):
            assert not fixture.psc(name).exists()
            assert not fixture.pex(name).exists()


def test_missing_required_addition_is_reported_to_native_reconciliation(tmp_path):
    with native_scripts(tmp_path, [(TRIGGER_SCRIPT, "ACTI")]) as fixture:
        for root in (fixture.source / "scripts/client", fixture.target / "Scripts"):
            write_pex(root / script_path(TRIGGER_SCRIPT, ".pex"), TRIGGER_SCRIPT)
        for path in (fixture.psc(TRIGGER_SCRIPT), fixture.pex(TRIGGER_SCRIPT)):
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"stale")
        report = fixture.convert()
        assert not fixture.psc(TRIGGER_SCRIPT).exists()
        assert not fixture.pex(TRIGGER_SCRIPT).exists()
        assert report["scripts"][0]["status"] == "addition_missing"
        assert (
            "required pair-scoped source artifact not found"
            in report["scripts"][0]["message"]
        )
        assert report["stripped_vmad"] == report["changed_records"] == 1
        assert fixture.references() == []
        assert fixture.saved_subrecords(0x800, "VMAD") == []


def test_addition_compile_failure_is_reported_to_native_reconciliation(tmp_path):
    with native_scripts(tmp_path, [(TRIGGER_SCRIPT, "ACTI")]) as fixture:
        fixture.addition(
            TRIGGER_SCRIPT,
            f"Scriptname {TRIGGER_SCRIPT}\nInt Function Broken()\nReturn missingValue\nEndFunction\n",
        )
        report = fixture.convert()
        assert not fixture.psc(TRIGGER_SCRIPT).exists()
        assert not fixture.pex(TRIGGER_SCRIPT).exists()
        assert report["scripts"][0]["status"] == "compile_failed"
        assert report["scripts"][0]["message"]
        assert report["stripped_vmad"] == report["changed_records"] == 1
        assert fixture.references() == []
        assert fixture.saved_subrecords(0x800, "VMAD") == []


def test_invalid_addition_artifact_is_reported_to_native_reconciliation(tmp_path):
    with native_scripts(tmp_path, [(TRIGGER_SCRIPT, "ACTI")]) as fixture:
        fixture.addition(TRIGGER_SCRIPT, "Scriptname B21:Wrong\n")
        report = fixture.convert()
        assert report["scripts"][0]["status"] == "addition_install_failed"
        assert (
            f"declares B21:Wrong, expected {TRIGGER_SCRIPT}"
            in report["scripts"][0]["message"]
        )
        assert report["stripped_vmad"] == report["changed_records"] == 1
        assert fixture.references() == []
        assert fixture.saved_subrecords(0x800, "VMAD") == []
