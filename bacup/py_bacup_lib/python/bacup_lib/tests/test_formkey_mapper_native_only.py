from __future__ import annotations

from bacup_lib.formkey.formkey_mapper import FormKeyMapper


class ExplodingLoader:
    def search_by_editor_id_and_type(self, editor_id: str, record_type: str):
        raise AssertionError("records DB loader must not be used during conversion")


def test_mapper_does_not_use_target_loader_when_handles_do_not_match(tmp_path) -> None:
    mapper = FormKeyMapper(
        mod_name="B21_Test",
        target_game="fo4",
        target_loader=ExplodingLoader(),
        target_master_handles=[],
        mod_path=str(tmp_path),
        use_base_game_assets=True,
        preserve_source_ids=True,
    )

    result = mapper.map_formkey(
        source_formkey="000800:Source.esp",
        editor_id="NoNativeMatch",
        record_type="WEAP",
        source_game="fo4",
    )

    assert result["strategy"] in {"source_id_preserved", "new_allocation"}
