"""Generate fixtures for the FNV -> FO4 weapon E2E test."""
from __future__ import annotations

import shutil
from pathlib import Path

import yaml

from creation_lib.animation.kf_writer import write_kf
from bacup_lib.models import AnimationClip, AnimationKeyframe, BoneChannel
from creation_lib.nif.nif_file import NifFile

HERE = Path(__file__).resolve().parent
SOURCE_NIF_DIR = HERE.parent / "nif" / "fnv" / "weapons"


def _ensure_dirs() -> None:
    for subdir in ("yaml", "Meshes/Weapons"):
        (HERE / subdir).mkdir(parents=True, exist_ok=True)


def _write_yaml(name: str, payload: dict) -> None:
    with (HERE / "yaml" / name).open("w", encoding="utf-8") as stream:
        yaml.safe_dump(payload, stream, sort_keys=False)


def _write_records() -> None:
    records = {
        "00_WeapNV10mmPistol.yaml": {
            "record_type": "WEAP",
            "eid": "WeapNV10mmPistol",
            "form_id": "0CAFE0",
            "fields": [
                {"FULL": {"TargetLanguage": "English", "Values": [{"Value": "10mm Pistol"}]}},
                {"AnimationType": "Pistol"},
                {"MODL": {"Filename": "Meshes/Weapons/10mmpistol.nif"}},
                {"ModelMod1": "Meshes/Weapons/10mmpistolExtMag.nif"},
                {"Ammo": "000123:FalloutNV.esm"},
                {"ModSlot1Linked": "IMODNV10mmExtMag"},
                {"AttackAnimation": {"Filename": "Meshes/Weapons/10mmpistolFire.kf"}},
            ],
        },
        "01_WeapNV357Revolver.yaml": {
            "record_type": "WEAP",
            "eid": "WeapNV357Revolver",
            "form_id": "0CAFE1",
            "fields": [
                {"FULL": {"TargetLanguage": "English", "Values": [{"Value": ".357 Revolver"}]}},
                {"AnimationType": "Pistol"},
                {"MODL": {"Filename": "Meshes/Weapons/357revolver.nif"}},
                {"Ammo": "000123:FalloutNV.esm"},
            ],
        },
        "02_WeapNVBaseballBat.yaml": {
            "record_type": "WEAP",
            "eid": "WeapNVBaseballBat",
            "form_id": "0CAFE2",
            "fields": [
                {"FULL": {"TargetLanguage": "English", "Values": [{"Value": "Baseball Bat"}]}},
                {"AnimationType": "Melee"},
                {"MODL": {"Filename": "Meshes/Weapons/baseballbat.nif"}},
            ],
        },
        "03_IMODNV10mmExtMag.yaml": {
            "record_type": "IMOD",
            "eid": "IMODNV10mmExtMag",
            "form_id": "0BEEF0",
            "fields": [
                {"FULL": "Extended Magazine"},
                {
                    "Data": {
                        "Value": 50,
                        "Weight": 0.2,
                        "Modifiers": [
                            {
                                "Field": "AmmoCapacity",
                                "Value": 6.0,
                                "Operation": "Add",
                            }
                        ],
                    }
                },
            ],
        },
        "04_Ammo10mm.yaml": {
            "record_type": "AMMO",
            "eid": "Ammo10mm",
            "form_id": "000123",
            "fields": [
                {"FULL": {"TargetLanguage": "English", "Values": [{"Value": "10mm Round"}]}},
            ],
        },
    }
    for name, payload in records.items():
        _write_yaml(name, payload)


def _write_nifs() -> None:
    mappings = {
        "10mmpistol.nif": SOURCE_NIF_DIR / "m2_min_base.nif",
        "10mmpistolExtMag.nif": SOURCE_NIF_DIR / "m2_min_with_attachment.nif",
        "357revolver.nif": SOURCE_NIF_DIR / "m2_min_base.nif",
        "baseballbat.nif": SOURCE_NIF_DIR / "m2_min_base.nif",
    }
    output_dir = HERE / "Meshes" / "Weapons"
    for filename, source_path in mappings.items():
        shutil.copyfile(source_path, output_dir / filename)
    baseball_bat = NifFile.load(str(output_dir / "baseballbat.nif"))
    root = baseball_bat.get_block(0)
    if root is not None:
        root.set_field("Name", "Weapon")
        baseball_bat.save()


def _write_kf() -> None:
    clip = AnimationClip(
        name="Fire",
        duration=1.0,
        channels=(
            BoneChannel(
                bone_name="Bip01 Magazine",
                rotations=(
                    AnimationKeyframe(time=0.0, value=(0.0, 0.0, 0.0, 1.0)),
                    AnimationKeyframe(time=1.0, value=(0.0, 0.0, 0.1, 0.995)),
                ),
            ),
            BoneChannel(
                bone_name="Bip01 Slide",
                rotations=(
                    AnimationKeyframe(time=0.0, value=(0.0, 0.0, 0.0, 1.0)),
                    AnimationKeyframe(time=1.0, value=(0.0, 0.1, 0.0, 0.995)),
                ),
            ),
        ),
        events=(),
    )
    write_kf(clip, HERE / "Meshes" / "Weapons" / "10mmpistolFire.kf", game="fnv")


def write_fixture() -> Path:
    _ensure_dirs()
    _write_records()
    _write_nifs()
    _write_kf()
    return HERE


if __name__ == "__main__":
    print(f"fixture written to {write_fixture()}")
