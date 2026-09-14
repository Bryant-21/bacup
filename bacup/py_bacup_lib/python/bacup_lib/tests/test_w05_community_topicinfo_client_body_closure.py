from __future__ import annotations

import csv
from pathlib import Path

from bacup_lib.workflows.unified import _script_patch_source


REPO_ROOT = Path(__file__).resolve().parents[5]
STATUS_PATH = REPO_ROOT / "bacup" / "docs" / "stub_restoration" / "status.csv"
CONTRACT = "contracts/w05-community-topicinfo-client-body-closure.md"

BODYLESS_TOPICINFO_CASES = (
    "Fragments:TopicInfos:TIF_W05_Community_BB_Quest_0054B0F7",
    "Fragments:TopicInfos:TIF_W05_Community_RaiderFish_0055B220",
    "Fragments:TopicInfos:TIF_W05_Community_RaiderFish_0055B238",
    "Fragments:TopicInfos:TIF_W05_Community_RaiderFish_0055B268",
    "Fragments:TopicInfos:TIF_W05_Community_RaiderFish_0057CEFA",
    "Fragments:TopicInfos:TIF_W05_Community_RaiderFish_0057CF4C",
    "Fragments:TopicInfos:TIF_W05_Community_RaiderFish_0057CF4D",
    "Fragments:TopicInfos:TIF_W05_Community_RaiderFish_0057CF4E",
    "Fragments:TopicInfos:TIF_W05_Community_RaiderFish_0057CF4F",
    "Fragments:TopicInfos:TIF_W05_Community_RaiderFish_0057CF62",
    "Fragments:TopicInfos:TIF_W05_Community_CottageBun_0055B228",
    "Fragments:TopicInfos:TIF_W05_Community_CottageBun_0055B23D",
    "Fragments:TopicInfos:TIF_W05_Community_RaiderBloc_00558E44",
    "Fragments:TopicInfos:TIF_W05_Community_RaiderBloc_00558E45",
    "Fragments:TopicInfos:TIF_W05_Community_RaiderBloc_00558E48",
    "Fragments:TopicInfos:TIF_W05_Community_RaiderBloc_00558E4A",
    "Fragments:TopicInfos:TIF_W05_Community_RaiderBloc_0055AE38",
    "Fragments:TopicInfos:TIF_W05_Community_RaiderBloc_0055AE3B",
    "Fragments:TopicInfos:TIF_W05_Community_RaiderBloc_0055AE3C",
    "Fragments:TopicInfos:TIF_W05_Community_Treehouse__00555AD7",
)


def test_community_bodyless_topicinfos_are_closed_without_invented_patches():
    with STATUS_PATH.open(encoding="utf-8", newline="") as status_file:
        rows = {row["script_name"].lower(): row for row in csv.DictReader(status_file)}

    for script_name in BODYLESS_TOPICINFO_CASES:
        row = rows[script_name.lower()]
        assert row["terminal_state"] == "non-defect"
        assert row["evidence"] == CONTRACT
        assert _script_patch_source(script_name) is None


def test_community_topicinfo_closure_contract_names_every_row():
    contract = (STATUS_PATH.parent / CONTRACT).read_text(encoding="utf-8").lower()

    assert len(BODYLESS_TOPICINFO_CASES) == 20
    for script_name in BODYLESS_TOPICINFO_CASES:
        form_id = script_name.rsplit("_", 1)[-1].lstrip("0").lower()
        assert form_id in contract
