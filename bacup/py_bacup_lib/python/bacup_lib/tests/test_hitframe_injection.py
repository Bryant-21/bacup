from bacup_lib.workflows import asset_phases as fixups
from creation_lib.hkxpack.model import HKXArrayMember, HKXDirectMember, HKXFile, HKXObject, HKXStringMember, HKXType


def _annotation(time: float, text: str) -> HKXObject:
    return HKXObject(
        name="",
        class_name="hkaAnnotationTrackAnnotation",
        members=[
            HKXDirectMember(name="time", type=HKXType.REAL, value=time),
            HKXStringMember(name="text", value=text, is_null=False),
        ],
    )


def test_inject_hitframe_events_treats_hitframe_case_insensitively(monkeypatch, tmp_path):
    attack_path = tmp_path / "attack1.hkx"
    attack_path.write_bytes(b"original")
    annotations = HKXArrayMember(
        "annotations",
        HKXType.STRUCT,
        [
            _annotation(0.1, "preHitFrame"),
            _annotation(0.2, "hitFrame"),
        ],
    )
    track = HKXObject(
        name="",
        class_name="hkaAnnotationTrack",
        members=[annotations],
    )
    hkx = HKXFile(
        objects=[
            HKXObject(
                name="#0001",
                class_name="hkaSplineCompressedAnimation",
                members=[HKXArrayMember("annotationTracks", HKXType.STRUCT, [track])],
            )
        ]
    )

    monkeypatch.setattr(
        "creation_lib._native.havok_native.load_hkx", lambda _path: (hkx, object())
    )
    monkeypatch.setattr(
        "creation_lib._native.havok_native.write_hkx", lambda _hkx, _reg: b"patched"
    )

    assert fixups._inject_hitframe_events(str(tmp_path)) == []
    assert [member.value for ann in annotations.contents for member in ann.members if member.name == "text"] == [
        "preHitFrame",
        "hitFrame",
    ]
    assert attack_path.read_bytes() == b"original"
