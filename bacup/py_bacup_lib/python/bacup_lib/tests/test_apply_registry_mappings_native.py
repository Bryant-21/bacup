from pathlib import Path

from bacup_lib.native_runtime import load_native_module
from bacup_lib.run import ConversionRun
from creation_lib.esp import Plugin
from creation_lib.esp.model import Record, Subrecord


def _write_holder_plugin(
    path: Path,
    game: str,
    *,
    include_resolved_master: bool = False,
) -> None:
    with Plugin.new(path.name, game=game) as plugin:
        plugin.add_master("Other.esp")
        if include_resolved_master:
            plugin.add_master("ResolvedOther.esp")
        plugin.add_record(
            Record(
                signature="WEAP",
                form_id=0x02000A00 if include_resolved_master else 0x01000A00,
                subrecords=[
                    Subrecord("EDID", b"RefHolder\0"),
                    Subrecord("INAM", (0x00000B00).to_bytes(4, "little"), "formid"),
                ],
            )
        )
        plugin.save(path)


def test_apply_registry_mappings_rewrites_references_in_run_target(
    tmp_path: Path,
) -> None:
    source_path = tmp_path / "source" / "Holder.esp"
    target_path = tmp_path / "target" / "Holder.esp"
    source_path.parent.mkdir()
    target_path.parent.mkdir()
    _write_holder_plugin(source_path, "fnv")
    _write_holder_plugin(target_path, "fo4", include_resolved_master=True)

    output_path = tmp_path / "output" / "Holder.esp"
    output_path.parent.mkdir()
    with ConversionRun.open_existing(
        "fnv",
        "fo4",
        str(source_path),
        str(target_path),
        config={"output_plugin_name": "Holder.esp"},
    ) as run:
        mappings = {"000B00:Other.esp": "000C00:ResolvedOther.esp"}
        count = load_native_module().conversion_run_apply_registry_mappings(
            run.id,
            mappings,
        )
        assert count == 1
        run.save_target(str(output_path), run_nvnm_validator=False)

    with Plugin.load(output_path, game="fo4") as plugin:
        refs = plugin.get_referenced_form_keys_by_subrecord(
            "Holder.esp:000A00",
            "INAM",
        )
    assert refs == ["ResolvedOther.esp:000C00"]
