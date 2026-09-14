from pathlib import Path

from bacup_lib.legacy_music import (
    augment_legacy_music_assets,
    discover_legacy_music_tracks,
    prepare_legacy_music_assets,
)
from bacup_lib.models import AssetProvenance, AssetRef


def _music_file(root: Path, relative: str, content: bytes = b"mp3") -> Path:
    path = root / "Music" / Path(relative)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(content)
    return path


def _sound_music_file(root: Path, relative: str, content: bytes = b"audio") -> Path:
    path = root / "Sound" / "fx" / "mus" / Path(relative)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(content)
    return path


def test_discovery_namespaces_primary_and_grafted_music(tmp_path: Path) -> None:
    fnv = tmp_path / "fnv" / "Data"
    fo3 = tmp_path / "fo3" / "Data"
    _music_file(fnv, "Explore/Same.mp3")
    _music_file(fo3, "Explore/Same.mp3")

    tracks = discover_legacy_music_tracks(
        source_game="fnv",
        target_game="fo4",
        output_plugin_name="FalloutNV.esm",
        primary_roots=(fnv,),
        additional_roots=(fo3,),
    )

    assert [track["asset_path"] for track in tracks] == [
        "Music/FalloutNV/FNV/Explore/Same.xwm",
        "Music/FalloutNV/FO3/Explore/Same.xwm",
    ]
    assert all(track["track_path"].startswith("Data\\Music\\") for track in tracks)


def test_discovery_includes_legacy_sound_music_and_transcodes_ogg(tmp_path: Path) -> None:
    fnv = tmp_path / "fnv" / "Data"
    source = _sound_music_file(fnv, "tenpenny/mus_tenpenny_01_lp.ogg")

    tracks = discover_legacy_music_tracks(
        source_game="fnv",
        target_game="fo4",
        output_plugin_name="FalloutNV.esm",
        primary_roots=(fnv,),
        additional_roots=(),
    )

    assert len(tracks) == 1
    assert Path(tracks[0]["source_path"]).samefile(source)
    assert tracks[0]["legacy_relative_path"] == "tenpenny/mus_tenpenny_01_lp.ogg"
    assert tracks[0]["asset_path"] == (
        "Music/FalloutNV/FNV/Sound/fx/mus/tenpenny/mus_tenpenny_01_lp.xwm"
    )
    # The record names .wav and the shipped asset is .xwm, matching every
    # vanilla FO4 MUST — a literal .xwm in the record does not resolve.
    assert tracks[0]["track_path"] == (
        "Data\\Music\\FalloutNV\\FNV\\Sound\\fx\\mus\\tenpenny\\"
        "mus_tenpenny_01_lp.wav"
    )


def test_augmentation_replaces_legacy_musc_directory_refs(tmp_path: Path) -> None:
    source = _music_file(tmp_path / "fnv", "Explore/Explore_01.mp3")
    directory_ref = AssetRef(
        asset_type="sound",
        source_path="Explore/",
        provenance=AssetProvenance("", "", "FNAM", 0, "native", "MUSC"),
    )
    tracks = [
        {
            "legacy_relative_path": "Explore/Explore_01.mp3",
            "source_path": str(source),
            "asset_path": "Music/Port/FNV/Explore/Explore_01.xwm",
            "track_path": "Data\\Music\\Port\\FNV\\Explore\\Explore_01.xwm",
        },
        {
            "legacy_relative_path": "Battle/Unused.mp3",
            "source_path": str(source),
            "asset_path": "Music/Port/FNV/Battle/Unused.xwm",
            "track_path": "Data\\Music\\Port\\FNV\\Battle\\Unused.xwm",
        },
    ]

    augmented = augment_legacy_music_assets([directory_ref], tracks)

    assert len(augmented) == 1
    assert augmented[0].source_path == tracks[0]["asset_path"]
    assert augmented[0].resolved_path == str(source)


def test_prepare_encodes_mp3_once_and_reuses_cache(tmp_path: Path) -> None:
    source = tmp_path / "source.mp3"
    source.write_bytes(b"mp3")
    asset = AssetRef(
        asset_type="sound",
        source_path="Music/Port/FNV/source.xwm",
        resolved_path=str(source),
    )
    calls: list[tuple[Path, Path]] = []

    def convert(input_path: Path, output_path: Path) -> None:
        calls.append((input_path, output_path))
        output_path.write_bytes(b"xwm")

    cache = tmp_path / "cache"
    first = prepare_legacy_music_assets([asset], cache, converter=convert)
    second = prepare_legacy_music_assets([asset], cache, converter=convert)

    assert len(calls) == 1
    assert first[0].resolved_path == second[0].resolved_path
    assert Path(first[0].resolved_path).read_bytes() == b"xwm"


def test_prepare_encodes_ogg(tmp_path: Path) -> None:
    source = tmp_path / "source.ogg"
    source.write_bytes(b"ogg")
    asset = AssetRef(
        asset_type="sound",
        source_path="Music/Port/FNV/source.xwm",
        resolved_path=str(source),
    )
    calls: list[tuple[Path, Path]] = []

    def convert(input_path: Path, output_path: Path) -> None:
        calls.append((input_path, output_path))
        output_path.write_bytes(b"xwm")

    prepared = prepare_legacy_music_assets([asset], tmp_path / "cache", converter=convert)

    assert calls[0][0] == source
    assert Path(prepared[0].resolved_path).read_bytes() == b"xwm"
