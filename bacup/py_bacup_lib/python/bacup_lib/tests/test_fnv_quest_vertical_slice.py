"""Binary FNV VTechatticup quest-slice coverage for the native tail."""

from __future__ import annotations

import json
from pathlib import Path

from bacup_lib.native_runtime import load_native_module
from bacup_lib.run import ConversionRun
from bacup_lib.source_pairs import (
    FNV_MVP_EXCLUDE_SIGNATURES,
    fnv_quest_slice_record_form_ids,
)
from creation_lib.esp import Plugin
from creation_lib.esp.api import export_data, import_json


FREEFORM_POWER_ARMOR_QUEST_ID = 0x06136D
QUEST_ID = 0x11F935
QUEST_SCRIPT_IDS = (0x11FC64, 0x123191, 0x134491, 0x166305)
FIRST_TOPIC_ID = 0x13015B
FIRST_INFO_ID = 0x130161
SECOND_TOPIC_ID = 0x134B9A
SECOND_INFO_ID = 0x134B9B
EMPTY_TOPIC_ID = 0x138A74
GREETING_TOPIC_ID = 0x0000C8
GREETING_INFO_IDS = (0x15734B, 0x15734C, 0x15734D)
HOSTAGE_ONE_ID = 0x12319B
HOSTAGE_TWO_ID = 0x12319C
RENOLDS_ID = 0x134B9C
RENOLDS_BASE_ID = 0x1300F0
TRIGGER_ID = 0x133F42
TRIGGER_BASE_ID = 0x133F41
ESCAPE_TARGET_BASE_ID = 0x045264
RENOLDS_PATROL_TARGET_BASE_ID = 0x000034
HOSTAGE_BASE_ID = 0x123193
ESCAPE_TARGET_ID = 0x0E70CA
RENOLDS_PATROL_TARGET_ID = 0x133F3D
HOSTAGE_VOICE_TYPE_ID = 0x02AB62
MAGIC_EFFECT_ID = 0x0CB05D
SPELL_ID = 0x172091
PERK_ID = 0x058FDF
POWER_ARMOR_MESSAGE_ID = 0x070A14
HOSTAGE_MESSAGE_ID = 0x1231B8
CAPTIVE_MESSAGE_ID = 0x097183
NCR_FACTION_ID = 0x0A46E7
LEGION_FACTION_ID = 0x1400F9
DONOR_ID = 0x05B307
DONOR_TEMPLATE_ID = 0x017BAB
OUTPUT_PREFIX = "FNV_FO3"
FO4_RESTORE_CONDITION_EFFECT_IDS = (
    0x05C529,
    0x05C52C,
    0x05C52D,
    0x05C52A,
    0x05C52B,
    0x05C52E,
)
NORMAL_SUPPORT_RECORD_IDS = {
    "MESG": (HOSTAGE_MESSAGE_ID, CAPTIVE_MESSAGE_ID, POWER_ARMOR_MESSAGE_ID),
    "FACT": (NCR_FACTION_ID, LEGION_FACTION_ID),
}
ORDINARY_TOPIC_EDITOR_IDS = {
    FIRST_TOPIC_ID: "VTechatticupFirstTopic",
    SECOND_TOPIC_ID: "VTechatticupSecondTopic",
    EMPTY_TOPIC_ID: "VTechatticupZeroInfoTopic",
}
PACK_EDITOR_IDS = {
    0x1231B6: "TecMineHostageEscape",
    0x1231B7: "TecMineHostagePackage",
    0x13289E: "TechaticupNCRRenoldsDialoguePackage",
    0x133F3E: "TechaticupNCRRenoldsPatrolPackage",
}


def _hex_text(value: str) -> str:
    return (value + "\0").encode().hex().upper()


def _hex_u32(value: int) -> str:
    return value.to_bytes(4, "little").hex().upper()


def _hex_script_source(value: str) -> str:
    return value.replace("\n", "\r\n").encode("cp1252").hex().upper()


LIVE_SCPT_SCTX = {
    0x11FC64: """scn VTechatticupQuestScript

;Quest Variables

Short NumHostages\t\t\t;Number of Hostages Freed
Short HostagesFreed\t\t\t;1=Player freed all hostages
Short HostagesDead\t\t\t;1=Hostages Dead
Short HostageStorVar\t\t\t;1=Renolds told the player about the hostages. 2= Player accepted the quest. 3= Quest complete
Short GreetingDone\t\t\t;1=Renolds Greeted the player
Short DoOnce\t\t\t\t\t;Prevent the script from repeating after completing the objective.

BEGIN GameMode
\tIf DoOnce ==1\t\t\t\t;Prevents script from repeating
\t\tReturn
\tElseif (NVTecNCRRenoldsREF.GetDead == 1)
\t\tsetStage VTechatticup 110
\tElseif (HostagesDead == 1)
\t\tset DoOnce to 1
\tElseif (NumHostages == 2) && (HostagesFreed !=1)
\t\tset HostagesFreed to 1\t
\tElseIf (HostagesFreed == 1) && (getStage VTechatticup == 10)
\t\tsetStage VTechatticup 20
\t\tset DoOnce to 1
\tEndIf
End""",
    0x123191: """scn TecMineHostage

Short \tFreed
Short \tButton
Short\tDoOnce
Short HostageFreedTotal\t;Total Hostages freed
ref\t\thostage

BEGIN OnLoad
\tif ( Freed == 1 )
\t\tdisable
\telse (  Freed == 0 )
\t\tIgnoreCrime 1
\t\tsetRestrained 1
\t\tset DoOnce to 0
\t\tsetRestrained 0
\tendif

\tset hostage to GetSelf

END


BEGIN OnActivate
\tif ( GetDead == 0 )
\t\tif ( freed == 0 )
\t\t\tif ( IsActionRef player == 1 )
\t\t\t\tif ( Player.IsInCombat == 0 )
\t\t\t\t\t\tShowMessage TecMineHostageMSG
\t\t\t\t\tendif
\t\t\t\telse
\t\t\t\t\tShowMessage FFSupermutantCaptiveNoActivateMessage
\t\t\t\tendif
\t\t\tendif
\t\tendif\t
\telseIf ( GetDead == 1 )
\t\tActivate
\tendif
END

BEGIN OnDeath
\tsetStage VTechatticup 110
\tset VTechatticup.HostagesDead to 1
END

Begin OnDeath player

\tif freed == 0\t
\t\tAddReputation RepNVNCR 0 3
\tendif

END

BEGIN GameMode

; Added conditional code to reduce the cost of running the script on every frame - unclear if these NPCs will run low-level processing. Part of a game-wide revision of scripts - Jorge 03/14/10

If GetInSameCell Player != 1
\tReturn
Else
\tif ( DoOnce == 0 )
\t\tif ( GetSitting == 3 )
\t\t\tset DoOnce to 1
\t\t\tsetRestrained 1
\t\tendif
\tendif
\tif ( freed == 0 )
\t\tset button to GetButtonPressed
\t\tif ( button == 1 )
\t\t\tSetRestrained 0
\t\t\tSayTo player GREETING
\t\t\tset Freed to 1
\t\t\tignoreCrime 0
\t\t\tAddToFaction NCRFactionNV 0
\t\t\tAddScriptPackage TecMineHostageEscape
\t\t\tset VTechatticup.NumHostages to VTechatticup.NumHostages + 1
\t\t\tSendAssaultAlarm Player CaesarsLegionTechMineFaction

\t\tendif
\tendif
Endif

END
""",
    0x134491: """scn NVTechatticupRenoldsDialogueScript

Begin OnTriggerEnter Player
\t
\tIf GetStage VTechatticup < 10
\t\tNVTecNCRRenoldsREF.AddScriptPackage TechaticupNCRRenoldsDialoguePackage
\tEndif
End""",
    0x166305: """scn NVTechNCRRenoldsSCRIPT

;Set Vtechatticup to Fail State if NCR Renolds dies.

Begin OnDeath NVTecNCRRenoldsREF

\tSetStage VTechatticup 110

END""",
}


LIVE_SCPT_RAW = {
    0x11FC64: (
        "0000000002000000D10000000C00000001000100",
        "1D000000100006000000C300000016000D000100090020730C002031203D3D1E0000001800120001000E0020720100582E1000002031203D3D39100A0002007202006E6E00000018000D000100090020730A002031203D3D15000700730C00020020311800190001001500207307002032203D3D20730800203120213D20262615000700730800020020311800210002001D00207308002031203D3D20583A1005000100720200203130203D3D20262639100A0002007202006E1400000015000700730C00020020311900000011000000",
    ),
    0x123191: (
        "000000000A0000009E0200000800000000000100",
        "1D0000001000060015006000000016000D0001000900207301002031203D3D221002000000170002000400AB10070001006E01000000F310070001006E010000001500070073020002002030F310070001006E000000001900000015000B0066070006002058CE10000011000000100008000200A7000000000016000F0009000B0020582E1000002030203D3D16000D0007000900207301002030203D3D160014000300100020586910050001007202002031203D3D1600120001000E002072020058211100002030203D3D59100B0001007203000000000000001900000017000200010059100B00010072040000000000000019000000190000001900000018000F0001000B0020582E1000002031203D3D0D10020000001900000011000000100008000A0020000000000039100A0002007205006E6E00000015000A00720500730A00020020311100000010000B000A002C000000010072020016000D0001000900207301002030203D3D39120F0003007206006E000000006E0300000019000000110000001000060000000C01000016001400010010002058201005000100720200203120213D1E00000017000200130016000D0004000900207302002030203D3D16000F0002000B0020589F1000002033203D3D1500070073020002002031F310070001006E01000000190000001900000016000D000B000900207301002030203D3D15000B00730500060020581F10000016000D0008000900207305002031203D3DF310070001006E000000003410080002007202007207001500070073010002002031AB10070001006E000000007F110A0002007208006E00000000971005000100720900150013007205007307000B00207205007307002031202B5F1008000200720200720A0019000000190000001900000011000000",
    ),
    0x134491: (
        "0000000004000000400000000000000000000100",
        "1D00000010000B001A002D0000000100720200160014000100100020583A1005000100720300203130203C1C0001009710050001007204001900000011000000",
    ),
    0x166305: (
        "0000000002000000250000000000000000000100",
        "1D00000010000B000A0012000000010072010039100A0002007202006E6E00000011000000",
    ),
}


LIVE_SCPT_LOCAL_FIELDS = {
    0x11FC64: (
        ("SLSD", "0700000000000000000000000000000001706C6574650D0A"),
        ("SCVR", "4E756D486F73746167657300"),
        ("SLSD", "080000000000000000000000000000000172656574656420"),
        ("SCVR", "486F737461676573467265656400"),
        ("SLSD", "0A00000000000000000000000000000001706C6574696E67"),
        ("SCVR", "486F7374616765734465616400"),
        ("SLSD", "09000000000000000000000000000000016D207265706561"),
        ("SCVR", "486F737461676553746F7256617200"),
        ("SLSD", "0B0000000000000000000000000000000163686174746963"),
        ("SCVR", "4772656574696E67446F6E6500"),
        ("SLSD", "0C00000000000000000000000000000001676573203D3D20"),
        ("SCVR", "446F4F6E636500"),
    ),
    0x123191: (
        ("SLSD", "010000000000000000000000000000000173746167654573"),
        ("SCVR", "467265656400"),
        ("SLSD", "050000000000000000000000000000000140000520000000"),
        ("SCVR", "427574746F6E00"),
        ("SLSD", "020000000000000000000000000000000100000000000000"),
        ("SCVR", "446F4F6E636500"),
        ("SLSD", "08000000000000000000000000000000016976654E6F4163"),
        ("SCVR", "486F73746167654672656564546F74616C00"),
        ("SLSD", "070000000000000000000000000000000000000000000000"),
        ("SCVR", "686F737461676500"),
        ("SCRV", "07000000"),
    ),
}


LIVE_SCPT_SCRO = {
    0x11FC64: (0x134B9C, QUEST_ID),
    0x123191: (
        0x000014,
        0x1231B8,
        0x097183,
        QUEST_ID,
        0x0F43DE,
        0x0000C8,
        0x0A46E7,
        0x1231B6,
        0x1400F9,
    ),
    0x134491: (0x134B9C, 0x000014, QUEST_ID, 0x13289E),
    0x166305: (0x134B9C, QUEST_ID),
}


def _record(
    signature: str,
    form_id: int,
    subrecords: list[tuple[str, str]],
    *,
    flags: str = "00000000",
    form_version: int = 15,
) -> dict:
    return {
        "signature": signature,
        "form_id": f"{form_id:06X}",
        "flags": flags,
        "form_version": form_version,
        "subrecords": [
            {"signature": field_signature, "data_hex": data_hex}
            for field_signature, data_hex in subrecords
        ],
    }


def _top_group(signature: str, children: list[dict]) -> dict:
    return {
        "type": "group",
        "label_text": signature,
        "group_type": 0,
        "children": children,
    }


def _child_group(form_id: int, group_type: int, children: list[dict]) -> dict:
    return {
        "type": "group",
        "label_hex": _hex_u32(form_id),
        "group_type": group_type,
        "children": children,
    }


def _script_record(form_id: int, editor_id: str) -> dict:
    schr, scda = LIVE_SCPT_RAW[form_id]
    scro = LIVE_SCPT_SCRO[form_id]
    return _record(
        "SCPT",
        form_id,
        [
            ("EDID", _hex_text(editor_id)),
            ("SCHR", schr),
            ("SCDA", scda),
            ("SCTX", _hex_script_source(LIVE_SCPT_SCTX[form_id])),
            *LIVE_SCPT_LOCAL_FIELDS.get(form_id, ()),
            *[("SCRO", _hex_u32(form_id)) for form_id in scro],
        ],
    )


def _info_record(form_id: int, *, two_responses: bool = False) -> dict:
    if form_id == SECOND_INFO_ID:
        return _record(
            "INFO",
            form_id,
            [
                ("DATA", "00000100"),
                ("QSTI", _hex_u32(QUEST_ID)),
                ("TRDT", "05000000140000000000000001C8E2000000000001000000"),
                (
                    "NAM1",
                    _hex_text(
                        "They must have run straight back to camp. I bet they didn't even stop and say thanks."
                    ),
                ),
                ("NAM2", "00"),
                ("NAM3", "00"),
                ("TRDT", "05000000140000000000000002E8E4000000000001000000"),
                (
                    "NAM1",
                    _hex_text(
                        "The NCR could use someone like you. Stop by our camps if you're looking for work."
                    ),
                ),
                ("NAM2", "00"),
                ("NAM3", "00"),
                ("CTDA", "60000000000020413A00000035F91100000000000000000000000000"),
                ("CTDA", "800000000000C8423A00000035F91100000000000000000000000000"),
                ("CTDA", "000000000000803F4F00000035F91100080000000000000000000000"),
                ("CTDA", "000000000000803F48000000F0001300000000000000000000000000"),
                ("SCHR", "0000000000000000000000000000000000000100"),
                ("NEXT", ""),
                ("SCHR", "00000000010000001C0000000000000000000100"),
                ("SCDA", "39100A0002007201006E6400000015000A0072010073090002002033"),
                (
                    "SCTX",
                    "736574537461676520565465636861747469637570203130300D0A736574207674656368617474696375702E486F737461676553746F7256617220746F2033",
                ),
                ("SCRO", _hex_u32(QUEST_ID)),
                ("ANAM", _hex_u32(RENOLDS_BASE_ID)),
            ],
            flags="00002000",
        )
    responses = [
        ("TRDT", "050000001400000000000000010000000000000001000000"),
        (
            "NAM1",
            _hex_text(
                "That's already more than I was expecting. I'll keep watch to make sure no more Legion come up the road. Good Luck."
            ),
        ),
        ("NAM2", "00"),
        ("NAM3", "00"),
    ]
    if two_responses:
        responses.extend(
            [
                ("TRDT", "050000001400000000000000020000000000000001000000"),
                ("NAM1", _hex_text("The NCR could use someone like you.")),
                ("NAM2", "00"),
                ("NAM3", "00"),
            ]
        )
    return _record(
        "INFO",
        form_id,
        [
            ("DATA", "00000100"),
            ("QSTI", _hex_u32(QUEST_ID)),
            *responses,
            ("CTDA", "000000000000803F48000000F0001300000000000000000000000000"),
            ("SCHR", "00000000010000001C0000000000000000000100"),
            ("SCDA", "39100A0002007201006E0A00000015000A0072010073090002002032"),
            (
                "SCTX",
                "7365747374616765207674656368617474696375702031300D0A736574207674656368617474696375702E486F737461676553746F7256617220746F2032",
            ),
            ("SCRO", _hex_u32(QUEST_ID)),
            ("NEXT", ""),
            ("SCHR", "0000000000000000000000000000000000000100"),
            ("ANAM", _hex_u32(RENOLDS_BASE_ID)),
        ],
        flags="00002000",
    )


def _greeting_info_record(
    form_id: int,
    *,
    text: str,
    data: str,
    response_data: str,
) -> dict:
    return _record(
        "INFO",
        form_id,
        [
            ("DATA", data),
            ("QSTI", _hex_u32(QUEST_ID)),
            ("TRDT", response_data),
            ("NAM1", _hex_text(text)),
            ("NAM2", "00"),
            ("NAM3", "00"),
            (
                "CTDA",
                "000000000000803F4800000093311200000000000000000000000000",
            ),
            ("SCHR", "0000000000000000000000000000000000000100"),
            ("NEXT", ""),
            ("SCHR", "0000000000000000000000000000000000000100"),
        ],
    )


def _pack_records() -> list[dict]:
    empty_event = [
        ("POBA", ""),
        ("INAM", "00000000"),
        ("SCHR", "0000000000000000000000000000000000000100"),
        ("TNAM", "00000000"),
        ("POEA", ""),
        ("INAM", "00000000"),
        ("SCHR", "0000000000000000000000000000000000000100"),
        ("TNAM", "00000000"),
        ("POCA", ""),
        ("INAM", "00000000"),
        ("SCHR", "0000000000000000000000000000000000000100"),
        ("TNAM", "00000000"),
    ]
    hostage_escape = _record(
        "PACK",
        0x1231B6,
        [
            ("EDID", _hex_text("TecMineHostageEscape")),
            ("PKDT", "063208040D67200000004454"),
            ("PLDT", "00000000CA700E0000000000"),
            ("PSDT", "FFFF00FF00000000"),
            ("PKPT", "0000"),
            ("POBA", ""),
            ("INAM", "00000000"),
            ("SCHR", "0000000000000000000000000000000000000100"),
            ("TNAM", "00000000"),
            ("POEA", ""),
            ("INAM", "00000000"),
            ("SCHR", "00000000010000000A0000000100000000000100"),
            ("SCDA", "1C000100221002000000"),
            ("SCTX", _hex_script_source("ref hostage\nhostage.disable")),
            ("SLSD", "01000000000000000000000000000000006A0E0408000000"),
            ("SCVR", _hex_text("hostage")),
            ("SCRV", "01000000"),
            ("TNAM", "00000000"),
            *empty_event[8:],
        ],
    )
    hostage_travel = _record(
        "PACK",
        0x1231B7,
        [
            ("EDID", _hex_text("TecMineHostagePackage")),
            ("PKDT", "061200040600200037006D03"),
            ("PLDT", "060000000000000000000000"),
            ("PSDT", "FFFF00FF00000000"),
            *empty_event,
        ],
    )
    renolds_dialogue = _record(
        "PACK",
        0x13289E,
        [
            ("EDID", _hex_text("TechaticupNCRRenoldsDialoguePackage")),
            ("PKDT", "002000000F00000000000000"),
            ("PSDT", "FFFF00FF00000000"),
            ("PTDT", "00000000140000008000000000000000"),
            ("CTDA", "80000000000020413A00000035F91100000000000000000000000000"),
            ("PKDD", "00009642C800000000010000000000000000000000000000"),
            *empty_event,
        ],
    )
    renolds_patrol = _record(
        "PACK",
        0x133F3E,
        [
            ("EDID", _hex_text("TechaticupNCRRenoldsPatrolPackage")),
            ("PKDT", "000000000D00000000000300"),
            ("PLDT", "000000003D3F130000000000"),
            ("PSDT", "FFFF00FF00000000"),
            ("PKPT", "0000"),
            *empty_event,
        ],
    )
    return [hostage_escape, hostage_travel, renolds_dialogue, renolds_patrol]


def _legacy_pack_origins() -> list[dict[str, str]]:
    return [
        {
            "merged_form_key": f"{form_id:06X}@FalloutNV.esm",
            "source_game": "fnv",
            "source_plugin": "FalloutNV.esm",
            "source_form_key": f"{form_id:08X}@FalloutNV.esm",
        }
        for form_id in (0x1231B6, 0x1231B7, 0x13289E, 0x133F3E)
    ]


def _source_payload(plugin_name: str) -> dict:
    freeform_quest = _record(
        "QUST",
        FREEFORM_POWER_ARMOR_QUEST_ID,
        [
            ("EDID", _hex_text("FreeformPowerArmor")),
            ("DATA", "1D3B000000000000"),
            (
                "CTDA",
                "0000000000000000C1010000DF8F0500000000000200000014000000",
            ),
            ("INDX", "0A00"),
            ("QSDT", "00"),
            ("SCHR", "0000000000000000000000000000000000000100"),
            ("SCTX", _hex_script_source(";Player has been directed to Gunny")),
            ("INDX", "1400"),
            ("QSDT", "00"),
            ("SCHR", "0000000000000000000000000000000000000100"),
            (
                "SCTX",
                _hex_script_source(";Player has been given permission from Lyons"),
            ),
            ("INDX", "6400"),
            ("QSDT", "00"),
            ("SCHR", "0000000003000000270000000000000000000100"),
            (
                "SCDA",
                "1C0001007611050001007202002912070001006E0100000059100B000100720300000000000000",
            ),
            (
                "SCTX",
                _hex_script_source(
                    ";Player is trained\nPlayer.AddPerk PowerArmorTraining \n"
                    "SetPCCanUsePowerArmor 1\nShowMessage PowerArmorTrainingPerkMsg"
                ),
            ),
            ("SCRO", _hex_u32(0x000014)),
            ("SCRO", _hex_u32(PERK_ID)),
            ("SCRO", _hex_u32(POWER_ARMOR_MESSAGE_ID)),
            ("INDX", "C800"),
            ("QSDT", "00"),
            ("SCHR", "0000000001000000090000000000000000000100"),
            ("SCDA", "371005000100720100"),
            (
                "SCTX",
                _hex_script_source(";Stop quest\nStopQuest FreeformPowerArmor"),
            ),
            ("SCRO", _hex_u32(FREEFORM_POWER_ARMOR_QUEST_ID)),
        ],
    )
    quest = _record(
        "QUST",
        QUEST_ID,
        [
            ("EDID", _hex_text("VTechatticup")),
            ("SCRI", _hex_u32(QUEST_SCRIPT_IDS[0])),
            ("FULL", _hex_text("Anywhere I Wander")),
            ("DATA", "1132E20000000000"),
            ("INDX", "0A00"),
            ("QSDT", "00"),
            ("SCHR", "0000000001000000130000000000000000000100"),
            ("SCDA", "A3110F0003007201006E0A0000006E01000000"),
            ("SCTX", _hex_text("SetObjectiveDisplayed VTechatticup 10 1\r\n")),
            ("SCRO", _hex_u32(QUEST_ID)),
            ("INDX", "1400"),
            ("QSDT", "00"),
            ("SCHR", "0000000001000000260000000000000000000100"),
            (
                "SCDA",
                "A2110F0003007201006E0A0000006E01000000A3110F0003007201006E140000006E01000000",
            ),
            (
                "SCTX",
                _hex_script_source(
                    "SetObjectiveCompleted Vtechatticup 10 1\n"
                    "SetObjectiveDisplayed VTechatticup 20 1\n"
                ),
            ),
            ("SCRO", _hex_u32(QUEST_ID)),
            ("INDX", "6400"),
            ("QSDT", "01"),
            ("CNAM", _hex_text("Quest Completed.")),
            ("SCHR", "00000000020000003A0000000000000000000100"),
            (
                "SCDA",
                "A2110F0003007201006E140000006E010000007711070001006E6400000039120F0003007202006E010000006E04000000711005000100720100",
            ),
            (
                "SCTX",
                _hex_script_source(
                    "SetObjectiveCompleted VTechatticup 20 1\n"
                    "RewardXP 100\n"
                    "AddReputation RepNVNCR 1 4 ; (11_7_10) Bug# 40881 -ETB\n"
                    "CompleteQuest Vtechatticup\n"
                    ";completeallobjectives vtechatticup\n"
                ),
            ),
            ("SCRO", _hex_u32(QUEST_ID)),
            ("SCRO", _hex_u32(0x0F43DE)),
            ("INDX", "6E00"),
            ("QSDT", "02"),
            ("CNAM", _hex_text("NCR hostages are dead.")),
            ("SCHR", "0000000001000000090000000000000000000100"),
            ("SCDA", "371005000100720100"),
            ("SCTX", _hex_script_source("StopQuest VMS20")),
            ("SCRO", _hex_u32(0x10E908)),
            ("QOBJ", "0A000000"),
            ("NNAM", _hex_text("Rescue the NCR hostages.")),
            ("QSTA", _hex_u32(HOSTAGE_ONE_ID) + "00F9241C"),
            ("QSTA", _hex_u32(HOSTAGE_TWO_ID) + "00F9241C"),
            ("QOBJ", "14000000"),
            ("NNAM", _hex_text("Report back to Private Renolds.")),
            ("QSTA", _hex_u32(RENOLDS_ID) + "00FA241C"),
            ("QOBJ", "6E000000"),
            ("NNAM", _hex_text("The NCR Hostages are dead.")),
        ],
    )
    first_topic = _record(
        "DIAL",
        FIRST_TOPIC_ID,
        [
            ("EDID", _hex_text("VTechatticupFirstTopic")),
            ("QSTI", _hex_u32(QUEST_ID)),
            ("FULL", _hex_text("I'll take a look.")),
            ("PNAM", "00004842"),
            ("DATA", "0000"),
        ],
    )
    second_topic = _record(
        "DIAL",
        SECOND_TOPIC_ID,
        [
            ("EDID", _hex_text("VTechatticupSecondTopic")),
            ("QSTI", _hex_u32(QUEST_ID)),
            ("FULL", _hex_text("Your friends are safe.")),
            ("PNAM", "00004842"),
            ("DATA", "0002"),
        ],
    )
    empty_topic = _record(
        "DIAL",
        EMPTY_TOPIC_ID,
        [
            ("EDID", _hex_text("VTechatticupZeroInfoTopic")),
            ("QSTI", _hex_u32(QUEST_ID)),
            ("FULL", _hex_text("I'm here about the hostages.")),
            ("PNAM", "00004842"),
            ("DATA", "0002"),
        ],
    )
    greeting_topic = _record(
        "DIAL",
        GREETING_TOPIC_ID,
        [
            ("EDID", _hex_text("GREETING")),
            ("QSTI", _hex_u32(QUEST_ID)),
            ("DATA", "0000"),
        ],
    )
    cell_id = 0x13A300
    persistent_children = _child_group(
        cell_id,
        8,
        [
            _record(
                "ACHR",
                HOSTAGE_ONE_ID,
                [("NAME", _hex_u32(HOSTAGE_BASE_ID))],
                flags="00000400",
            ),
            _record(
                "ACHR",
                HOSTAGE_TWO_ID,
                [("NAME", _hex_u32(HOSTAGE_BASE_ID))],
                flags="00000400",
            ),
            _record(
                "ACHR",
                RENOLDS_ID,
                [("NAME", _hex_u32(RENOLDS_BASE_ID))],
                flags="00000400",
            ),
            _record(
                "REFR",
                TRIGGER_ID,
                [
                    ("EDID", _hex_text("TecRenoldsDialogueActREF")),
                    ("NAME", _hex_u32(TRIGGER_BASE_ID)),
                    (
                        "XPRM",
                        "00001C440000814400003043CDCC4C3F9A99993ECDCC4C3E9A99193E01000000",
                    ),
                    ("XMBO", "00001C440000814400003043"),
                    ("DATA", "00905647002039C700C09445000000000000000023861840"),
                ],
                flags="00000400",
            ),
            _record(
                "REFR",
                ESCAPE_TARGET_ID,
                [("NAME", _hex_u32(ESCAPE_TARGET_BASE_ID))],
                flags="00000400",
            ),
            _record(
                "REFR",
                RENOLDS_PATROL_TARGET_ID,
                [("NAME", _hex_u32(RENOLDS_PATROL_TARGET_BASE_ID))],
                flags="00000400",
            ),
        ],
    )
    magic_effect = _record(
        "MGEF",
        MAGIC_EFFECT_ID,
        [
            ("EDID", _hex_text("RestoreAllLimbs")),
            ("FULL", _hex_text("Restore All Body Parts")),
            ("DESC", "00"),
            (
                "DATA",
                "30200000000000007FB00C00FFFFFFFFFFFFFFFF00003A20000000000000803F000000000000000000000000000000000000000000000000000000000000000001000000FFFFFFFF",
            ),
        ],
    )
    spell = _record(
        "SPEL",
        SPELL_ID,
        [
            ("EDID", _hex_text("DoctorLimbRestoration")),
            ("FULL", _hex_text("Limb Restoration")),
            ("SPIT", "00000000000000000000000000000000"),
            ("EFID", _hex_u32(MAGIC_EFFECT_ID)),
            ("EFIT", "0A000000000000000000000000000000FFFFFFFF"),
        ],
    )
    perk = _record(
        "PERK",
        PERK_ID,
        [
            ("EDID", _hex_text("PowerArmorTraining")),
            ("FULL", _hex_text("Power Armor Training")),
            ("DATA", "0000010000"),
        ],
    )
    hostage_message = _record(
        "MESG",
        HOSTAGE_MESSAGE_ID,
        [
            ("EDID", _hex_text("TecMineHostageMSG")),
            ("DESC", "00"),
            ("INAM", _hex_u32(0)),
            ("DNAM", "01000000"),
            ("ITXT", _hex_text("Leave him tied up.")),
            ("ITXT", _hex_text("Untie him.")),
        ],
    )
    captive_message = _record(
        "MESG",
        CAPTIVE_MESSAGE_ID,
        [
            ("EDID", _hex_text("FFSupermutantCaptiveNoActivateMessage")),
            ("DESC", _hex_text("You cannot help the captive while in combat.")),
            ("INAM", _hex_u32(0)),
            ("DNAM", "00000000"),
            ("TNAM", "00002041"),
        ],
    )
    power_armor_message = _record(
        "MESG",
        POWER_ARMOR_MESSAGE_ID,
        [
            ("EDID", _hex_text("PowerArmorTrainingPerkMsg")),
            (
                "DESC",
                _hex_text(
                    "You have received the specialized training needed to move in any form of Power Armor."
                ),
            ),
            ("FULL", _hex_text("Perk Added")),
            ("INAM", _hex_u32(0)),
            ("DNAM", "01000000"),
        ],
    )
    ncr_faction = _record(
        "FACT",
        NCR_FACTION_ID,
        [
            ("EDID", _hex_text("NCRFactionNV")),
            ("FULL", _hex_text("New California Republic")),
            ("DATA", "00010000"),
        ],
    )
    legion_faction = _record(
        "FACT",
        LEGION_FACTION_ID,
        [
            ("EDID", _hex_text("CaesarsLegionTechMineFaction")),
            ("FULL", _hex_text("Caesars Legion Techatticup Faction")),
            ("DATA", "03010000"),
        ],
    )
    return {
        "plugin": plugin_name,
        "game": "fnv",
        "header_size": 24,
        "header": {
            "version": 1.34,
            "next_object_id": "180000",
        },
        "items": [
            _top_group("QUST", [freeform_quest, quest]),
            _top_group(
                "SCPT",
                [
                    _script_record(QUEST_SCRIPT_IDS[0], "VTechatticupQuestScript"),
                    _script_record(QUEST_SCRIPT_IDS[1], "TecMineHostage"),
                    _script_record(
                        QUEST_SCRIPT_IDS[2], "NVTechatticupRenoldsDialogueScript"
                    ),
                    _script_record(QUEST_SCRIPT_IDS[3], "NVTechNCRRenoldsSCRIPT"),
                ],
            ),
            _top_group(
                "DIAL",
                [
                    greeting_topic,
                    _child_group(
                        GREETING_TOPIC_ID,
                        7,
                        [
                            _greeting_info_record(
                                GREETING_INFO_IDS[0],
                                text="I'm getting out of here.",
                                data="00000300",
                                response_data="030000001400000000000000010000000000000001000000",
                            ),
                            _greeting_info_record(
                                GREETING_INFO_IDS[1],
                                text="Fuck this place.",
                                data="00000300",
                                response_data="010000001400000000000000014541540000000001200000",
                            ),
                            _greeting_info_record(
                                GREETING_INFO_IDS[2],
                                text="The Legion will pay for this.",
                                data="00002300",
                                response_data="010000001400000000000000012046750000000001696E20",
                            ),
                        ],
                    ),
                    first_topic,
                    _child_group(FIRST_TOPIC_ID, 7, [_info_record(FIRST_INFO_ID)]),
                    second_topic,
                    _child_group(
                        SECOND_TOPIC_ID,
                        7,
                        [_info_record(SECOND_INFO_ID, two_responses=True)],
                    ),
                    empty_topic,
                ],
            ),
            _top_group(
                "NPC_",
                [
                    _record(
                        "NPC_",
                        HOSTAGE_BASE_ID,
                        [
                            ("EDID", _hex_text("TechMineNCRHostage")),
                            ("VTCK", _hex_u32(HOSTAGE_VOICE_TYPE_ID)),
                            ("SCRI", _hex_u32(QUEST_SCRIPT_IDS[1])),
                        ],
                    ),
                    _record(
                        "NPC_",
                        RENOLDS_BASE_ID,
                        [
                            ("EDID", _hex_text("NVTechatticupNCRRenolds")),
                            ("VTCK", _hex_u32(HOSTAGE_VOICE_TYPE_ID)),
                            ("SCRI", _hex_u32(QUEST_SCRIPT_IDS[3])),
                            ("PKID", _hex_u32(0x133F3E)),
                            ("PKID", _hex_u32(0x13289E)),
                        ],
                    ),
                ],
            ),
            _top_group(
                "VTYP",
                [
                    _record(
                        "VTYP",
                        HOSTAGE_VOICE_TYPE_ID,
                        [("EDID", _hex_text("MaleAdult01DefaultB"))],
                    ),
                ],
            ),
            _top_group(
                "ACTI",
                [
                    _record(
                        "ACTI",
                        TRIGGER_BASE_ID,
                        [
                            ("EDID", _hex_text("TecRenoldsDialogueActivator")),
                            ("OBND", "90FDF8FB50FF70020804B000"),
                            ("FULL", _hex_text("Renolds Dialogue Activator")),
                            ("SCRI", _hex_u32(QUEST_SCRIPT_IDS[2])),
                        ],
                    )
                ],
            ),
            _top_group(
                "CELL",
                [
                    _record("CELL", cell_id, [("EDID", _hex_text("VTechatticupCell"))]),
                    _child_group(cell_id, 6, [persistent_children]),
                ],
            ),
            _top_group("PACK", _pack_records()),
            _top_group("MGEF", [magic_effect]),
            _top_group("SPEL", [spell]),
            _top_group("PERK", [perk]),
            _top_group("MESG", [hostage_message, captive_message, power_armor_message]),
            _top_group("FACT", [ncr_faction, legion_faction]),
        ],
    }


def _force_greet_donor_payload(plugin_name: str) -> dict:
    return {
        "plugin": plugin_name,
        "game": "fo4",
        "header": {"version": 1.0, "next_object_id": "060000"},
        "items": [
            _top_group(
                "PACK",
                [
                    _record(
                        "PACK",
                        DONOR_ID,
                        [
                            ("EDID", _hex_text("RETravelCC01_Forcegreet")),
                            ("PKDT", "002000001200010000FC0000"),
                            ("PSDT", "00FF00FFFF00000000000000"),
                            (
                                "CTDA",
                                "00AD34410000A0413A0000003A760400000000000000000000000000FFFFFFFF",
                            ),
                            ("QNAM", "3A760400"),
                            ("PKCU", "18000000AB7B01000B000000"),
                            ("ANAM", _hex_text("Topic")),
                            ("PDTO", "0100000048454C4F"),
                            ("ANAM", _hex_text("Location")),
                            ("PLDT", "0C000000000000000001000000000000"),
                            ("ANAM", _hex_text("Location")),
                            ("PLDT", "0C00000000000000B80B000000000000"),
                            ("ANAM", _hex_text("Location")),
                            ("PLDT", "00000000140000008000000000000000"),
                            ("ANAM", _hex_text("Bool")),
                            ("CNAM", "01"),
                            ("ANAM", _hex_text("SingleRef")),
                            ("PTDA", "000000001400000000000000"),
                            ("ANAM", _hex_text("Location")),
                            ("PLDT", "00000000140000008813000000000000"),
                            *[
                                field
                                for value in [1, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 0]
                                for field in [
                                    ("ANAM", _hex_text("Bool")),
                                    ("CNAM", f"{value:02X}"),
                                ]
                            ],
                            ("ANAM", _hex_text("TargetSelector")),
                            ("PTDA", "020000001400000000000000"),
                            ("ANAM", _hex_text("Float")),
                            ("CNAM", "00000000"),
                            ("ANAM", _hex_text("Int")),
                            ("CNAM", "00000000"),
                            ("ANAM", _hex_text("TargetSelector")),
                            ("PTDA", "020000000000000000000000"),
                            *[
                                ("UNAM", value)
                                for value in [
                                    "00",
                                    "01",
                                    "02",
                                    "05",
                                    "07",
                                    "04",
                                    "09",
                                    "0D",
                                    "0F",
                                    "10",
                                    "11",
                                    "12",
                                    "13",
                                    "14",
                                    "1A",
                                    "16",
                                    "18",
                                    "1C",
                                    "1D",
                                    "1F",
                                    "20",
                                    "21",
                                    "22",
                                    "24",
                                ]
                            ],
                            ("XNAM", "0C"),
                            ("POBA", ""),
                            ("INAM", "00000000"),
                            ("PDTO", "0000000000000000"),
                            ("POEA", ""),
                            ("INAM", "00000000"),
                            ("PDTO", "0000000000000000"),
                            ("POCA", ""),
                            ("INAM", "00000000"),
                            ("PDTO", "0000000000000000"),
                        ],
                        form_version=127,
                    )
                ],
            )
        ],
    }


def _records(items: list[dict], signature: str) -> list[dict]:
    found: list[dict] = []
    for item in items:
        if item.get("type") == "group":
            found.extend(_records(item["children"], signature))
        elif item.get("signature") == signature:
            found.append(item)
    return found


def _record_by_id(items: list[dict], signature: str, form_id: int) -> dict:
    expected = f"{form_id:06X}"
    return next(
        record for record in _records(items, signature) if record["form_id"] == expected
    )


def _record_by_editor_id(items: list[dict], signature: str, editor_id: str) -> dict:
    return next(
        record
        for record in _records(items, signature)
        if record.get("editor_id") == editor_id
    )


def _top_group_by_signature(items: list[dict], signature: str) -> dict:
    return next(
        item
        for item in items
        if item.get("type") == "group"
        and item.get("group_type") == 0
        and item.get("label_text") == signature
    )


def _subrecord_data(record: dict, signature: str) -> list[str]:
    return [
        field["data_hex"]
        for field in record["subrecords"]
        if field["signature"] == signature
    ]


def _form_id_local(data_hex: str) -> int:
    return int.from_bytes(bytes.fromhex(data_hex[:8]), "little") & 0x00FF_FFFF


def _subrecord_form_id_locals(record: dict, signature: str) -> list[int]:
    return [_form_id_local(data_hex) for data_hex in _subrecord_data(record, signature)]


def _psc_property_names(source: str) -> list[str]:
    return [
        tokens[2]
        for line in source.splitlines()
        if len(tokens := line.split()) >= 3 and tokens[1] == "Property"
    ]


def _topic_info_group(topic_id: int, dial_group: dict) -> dict:
    return next(
        child
        for child in dial_group["children"]
        if child.get("type") == "group"
        and child.get("group_type") == 7
        and _form_id_local(child["label_hex"]) == topic_id
    )


def _assert_selected_achrs_are_persistent_cell_children(items: list[dict]) -> None:
    assert not any(
        item.get("type") == "group"
        and item.get("group_type") == 0
        and item.get("label_text") == "ACHR"
        for item in items
    )
    cell_group = _top_group_by_signature(items, "CELL")
    persistent_groups = [
        child
        for cell_child in cell_group["children"]
        if cell_child.get("type") == "group" and cell_child.get("group_type") == 6
        for child in cell_child["children"]
        if child.get("type") == "group" and child.get("group_type") == 8
    ]
    persistent_achrs = [
        record
        for group in persistent_groups
        for record in _records(group["children"], "ACHR")
    ]
    assert sorted(record["form_id"] for record in persistent_achrs) == [
        f"{form_id:06X}" for form_id in (HOSTAGE_ONE_ID, HOSTAGE_TWO_ID, RENOLDS_ID)
    ]
    assert all(record["flags"] == "00000400" for record in persistent_achrs)


def _assert_exact_audited_output_closure(
    items: list[dict], slice_records: dict[str, list[int]]
) -> None:
    for signature in ("QUST", "INFO", "PACK", "SPEL", "PERK", "MGEF"):
        assert {
            int(record["form_id"], 16) for record in _records(items, signature)
        } == set(slice_records[signature])
    expected_source_topics = set(slice_records["DIAL"]) - {GREETING_TOPIC_ID}
    actual_topics = _records(items, "DIAL")
    assert {
        int(record["form_id"], 16)
        for record in actual_topics
        if int(record["form_id"], 16) in expected_source_topics
    } == expected_source_topics
    assert len(actual_topics) == len(expected_source_topics) + 1
    for signature, form_ids in NORMAL_SUPPORT_RECORD_IDS.items():
        assert {
            int(record["form_id"], 16) for record in _records(items, signature)
        } == set(form_ids)
    assert len(_records(items, "SCEN")) == 1
    assert len(_records(items, "DLBR")) == 1


def _write_plugin(path: Path, payload: dict) -> Path:
    with import_json(json.dumps(payload)) as plugin:
        plugin.save(path)
    return path


def test_fnv_quest_vertical_slice_survives_native_tail_and_reopen(
    tmp_path: Path,
) -> None:
    slice_records = fnv_quest_slice_record_form_ids()
    assert slice_records["ACHR"] == [HOSTAGE_ONE_ID, HOSTAGE_TWO_ID, RENOLDS_ID]
    assert slice_records["MGEF"] == [MAGIC_EFFECT_ID]
    assert slice_records["PACK"] == [0x1231B6, 0x1231B7, 0x13289E, 0x133F3E]

    source_payload = _source_payload("FalloutNV.esm")
    assert source_payload["header_size"] == 24
    source_path = _write_plugin(tmp_path / "FalloutNV.esm", source_payload)
    donor_path = _write_plugin(
        tmp_path / "Fallout4.esm",
        _force_greet_donor_payload("Fallout4.esm"),
    )
    with Plugin.load(source_path, game="fnv") as source:
        source_export = export_data(source)
    with Plugin.load(donor_path, game="fo4") as donor:
        donor_export = export_data(donor)

    source_items = source_export["items"]
    assert source_export["plugin"] == "FalloutNV.esm"
    assert not _records(source_items, "SCEN")
    source_quest = _record_by_id(source_items, "QUST", QUEST_ID)
    assert _subrecord_data(source_quest, "SCRI") == [_hex_u32(QUEST_SCRIPT_IDS[0])]
    assert _subrecord_data(source_quest, "QSTA") == [
        _hex_u32(HOSTAGE_ONE_ID) + "00F9241C",
        _hex_u32(HOSTAGE_TWO_ID) + "00F9241C",
        _hex_u32(RENOLDS_ID) + "00FA241C",
    ]
    assert _subrecord_data(source_quest, "SCTX")
    assert _subrecord_data(source_quest, "SCDA")
    assert _hex_script_source(
        "SetObjectiveCompleted VTechatticup 20 1\n"
        "RewardXP 100\n"
        "AddReputation RepNVNCR 1 4 ; (11_7_10) Bug# 40881 -ETB\n"
        "CompleteQuest Vtechatticup\n"
        ";completeallobjectives vtechatticup\n"
    ) in _subrecord_data(source_quest, "SCTX")
    assert _subrecord_data(
        _record_by_id(source_items, "QUST", FREEFORM_POWER_ARMOR_QUEST_ID), "SCRO"
    ) == [
        _hex_u32(0x000014),
        _hex_u32(PERK_ID),
        _hex_u32(POWER_ARMOR_MESSAGE_ID),
        _hex_u32(FREEFORM_POWER_ARMOR_QUEST_ID),
    ]
    assert _subrecord_data(
        _record_by_id(source_items, "QUST", FREEFORM_POWER_ARMOR_QUEST_ID), "QSDT"
    ) == ["00", "00", "00", "00"]
    assert _subrecord_data(source_quest, "QSDT") == ["00", "00", "01", "02"]
    for script_id in QUEST_SCRIPT_IDS:
        source_script = _record_by_id(source_items, "SCPT", script_id)
        assert [field["signature"] for field in source_script["subrecords"]] == [
            "EDID",
            "SCHR",
            "SCDA",
            "SCTX",
            *[signature for signature, _ in LIVE_SCPT_LOCAL_FIELDS.get(script_id, ())],
            *["SCRO"] * len(LIVE_SCPT_SCRO[script_id]),
        ]
        assert _subrecord_data(source_script, "SCTX") == [
            _hex_script_source(LIVE_SCPT_SCTX[script_id])
        ]
        assert _subrecord_data(source_script, "SCHR") == [LIVE_SCPT_RAW[script_id][0]]
        assert _subrecord_data(source_script, "SCDA") == [LIVE_SCPT_RAW[script_id][1]]
        assert _subrecord_data(source_script, "SCRO") == [
            _hex_u32(form_id) for form_id in LIVE_SCPT_SCRO[script_id]
        ]
    assert _subrecord_data(_record_by_id(source_items, "SCPT", 0x123191), "SCRO") == [
        _hex_u32(form_id)
        for form_id in (
            0x000014,
            0x1231B8,
            0x097183,
            QUEST_ID,
            0x0F43DE,
            0x0000C8,
            0x0A46E7,
            0x1231B6,
            0x1400F9,
        )
    ]

    source_dial_group = _top_group_by_signature(source_items, "DIAL")
    for topic_id in (
        GREETING_TOPIC_ID,
        FIRST_TOPIC_ID,
        SECOND_TOPIC_ID,
        EMPTY_TOPIC_ID,
    ):
        assert _record_by_id(source_dial_group["children"], "DIAL", topic_id)
    source_greeting_infos = _topic_info_group(GREETING_TOPIC_ID, source_dial_group)[
        "children"
    ]
    assert [
        record["form_id"] for record in _records(source_greeting_infos, "INFO")
    ] == [f"{form_id:06X}" for form_id in GREETING_INFO_IDS]
    assert [
        _subrecord_data(_record_by_id(source_greeting_infos, "INFO", form_id), "NAM1")
        for form_id in GREETING_INFO_IDS
    ] == [
        [_hex_text("I'm getting out of here.")],
        [_hex_text("Fuck this place.")],
        [_hex_text("The Legion will pay for this.")],
    ]
    for form_id in GREETING_INFO_IDS:
        greeting_info = _record_by_id(source_greeting_infos, "INFO", form_id)
        assert _subrecord_data(greeting_info, "QSTI") == [_hex_u32(QUEST_ID)]
        assert _subrecord_data(greeting_info, "CTDA") == [
            "000000000000803F4800000093311200000000000000000000000000"
        ]
        assert not _subrecord_data(greeting_info, "PNAM")
        assert not _subrecord_data(greeting_info, "ANAM")
    assert [
        _subrecord_data(_record_by_id(source_greeting_infos, "INFO", form_id), "DATA")
        for form_id in GREETING_INFO_IDS
    ] == [["00000300"], ["00000300"], ["00002300"]]
    assert [
        _subrecord_data(_record_by_id(source_greeting_infos, "INFO", form_id), "TRDT")
        for form_id in GREETING_INFO_IDS
    ] == [
        ["030000001400000000000000010000000000000001000000"],
        ["010000001400000000000000014541540000000001200000"],
        ["010000001400000000000000012046750000000001696E20"],
    ]
    assert _record_by_id(
        _topic_info_group(FIRST_TOPIC_ID, source_dial_group)["children"],
        "INFO",
        FIRST_INFO_ID,
    )
    source_second_info = _record_by_id(
        _topic_info_group(SECOND_TOPIC_ID, source_dial_group)["children"],
        "INFO",
        SECOND_INFO_ID,
    )
    assert len(_subrecord_data(source_second_info, "TRDT")) == 2
    assert len(_subrecord_data(source_second_info, "NAM1")) == 2
    assert [field["signature"] for field in source_second_info["subrecords"][-7:]] == [
        "SCHR",
        "NEXT",
        "SCHR",
        "SCDA",
        "SCTX",
        "SCRO",
        "ANAM",
    ]
    assert _subrecord_data(source_second_info, "SCTX") == [
        "736574537461676520565465636861747469637570203130300D0A736574207674656368617474696375702E486F737461676553746F7256617220746F2033"
    ]
    assert not any(
        child.get("type") == "group"
        and child.get("group_type") == 7
        and _form_id_local(child["label_hex"]) == EMPTY_TOPIC_ID
        for child in source_dial_group["children"]
    )

    for form_id in (HOSTAGE_ONE_ID, HOSTAGE_TWO_ID, RENOLDS_ID, TRIGGER_ID):
        signature = "REFR" if form_id == TRIGGER_ID else "ACHR"
        assert _record_by_id(source_items, signature, form_id)["flags"] == "00000400"
    assert _subrecord_data(_record_by_id(source_items, "REFR", TRIGGER_ID), "NAME") == [
        _hex_u32(TRIGGER_BASE_ID)
    ]
    source_trigger = _record_by_id(source_items, "REFR", TRIGGER_ID)
    assert source_trigger["editor_id"] == "TecRenoldsDialogueActREF"
    assert _subrecord_data(source_trigger, "XPRM") == [
        "00001C440000814400003043CDCC4C3F9A99993ECDCC4C3E9A99193E01000000"
    ]
    assert _subrecord_data(source_trigger, "XMBO") == ["00001C440000814400003043"]
    assert _subrecord_data(source_trigger, "DATA") == [
        "00905647002039C700C09445000000000000000023861840"
    ]
    assert _subrecord_form_id_locals(
        _record_by_id(source_items, "REFR", ESCAPE_TARGET_ID), "NAME"
    ) == [ESCAPE_TARGET_BASE_ID]
    assert _subrecord_form_id_locals(
        _record_by_id(source_items, "REFR", RENOLDS_PATROL_TARGET_ID), "NAME"
    ) == [RENOLDS_PATROL_TARGET_BASE_ID]
    source_trigger_base = _record_by_id(source_items, "ACTI", TRIGGER_BASE_ID)
    assert source_trigger_base["editor_id"] == "TecRenoldsDialogueActivator"
    assert _subrecord_data(source_trigger_base, "OBND") == ["90FDF8FB50FF70020804B000"]
    assert _subrecord_data(source_trigger_base, "FULL") == [
        _hex_text("Renolds Dialogue Activator")
    ]
    assert _subrecord_data(
        _record_by_id(source_items, "ACTI", TRIGGER_BASE_ID), "SCRI"
    ) == [_hex_u32(QUEST_SCRIPT_IDS[2])]
    assert _subrecord_data(
        _record_by_id(source_items, "NPC_", HOSTAGE_BASE_ID), "VTCK"
    ) == [_hex_u32(HOSTAGE_VOICE_TYPE_ID)]
    assert _subrecord_data(
        _record_by_id(source_items, "NPC_", HOSTAGE_BASE_ID), "SCRI"
    ) == [_hex_u32(QUEST_SCRIPT_IDS[1])]
    assert _subrecord_data(
        _record_by_id(source_items, "NPC_", RENOLDS_BASE_ID), "SCRI"
    ) == [_hex_u32(QUEST_SCRIPT_IDS[3])]
    assert _subrecord_data(
        _record_by_id(source_items, "NPC_", RENOLDS_BASE_ID), "PKID"
    ) == [_hex_u32(0x133F3E), _hex_u32(0x13289E)]
    assert (
        _record_by_id(source_items, "VTYP", HOSTAGE_VOICE_TYPE_ID)["editor_id"]
        == "MaleAdult01DefaultB"
    )
    for signature, form_ids in NORMAL_SUPPORT_RECORD_IDS.items():
        assert {
            int(record["form_id"], 16) for record in _records(source_items, signature)
        } == set(form_ids)
    assert _subrecord_data(_record_by_id(source_items, "SPEL", SPELL_ID), "EFID") == [
        _hex_u32(MAGIC_EFFECT_ID)
    ]
    assert _record_by_id(source_items, "MGEF", MAGIC_EFFECT_ID)["form_id"] == "0CB05D"
    donor_pack = _record_by_id(donor_export["items"], "PACK", DONOR_ID)
    assert donor_pack["editor_id"] == "RETravelCC01_Forcegreet"
    assert len(_subrecord_data(donor_pack, "ANAM")) == 24
    assert _subrecord_data(donor_pack, "PKCU") == ["18000000AB7B01000B000000"]
    assert _subrecord_data(donor_pack, "PLDT") == [
        "0C000000000000000001000000000000",
        "0C00000000000000B80B000000000000",
        "00000000140000008000000000000000",
        "00000000140000008813000000000000",
    ]
    assert _subrecord_data(donor_pack, "PTDA") == [
        "000000001400000000000000",
        "020000001400000000000000",
        "020000000000000000000000",
    ]
    assert _subrecord_data(donor_pack, "UNAM") == [
        "00",
        "01",
        "02",
        "05",
        "07",
        "04",
        "09",
        "0D",
        "0F",
        "10",
        "11",
        "12",
        "13",
        "14",
        "1A",
        "16",
        "18",
        "1C",
        "1D",
        "1F",
        "20",
        "21",
        "22",
        "24",
    ]

    target_path = tmp_path / "FNVQuestVerticalSliceOutput.esm"
    config = {
        "mod_path": str(tmp_path),
        "is_whole_plugin": True,
        "preserve_source_ids": True,
        "strict_mapper": True,
        "skip_record_signatures": sorted(FNV_MVP_EXCLUDE_SIGNATURES),
        "fnv_quest_slice": True,
        "fnv_quest_slice_records": slice_records,
        "legacy_runtime_origins": [
            {
                "signature": signature,
                "merged_form_key": f"{form_id:06X}@FalloutNV.esm",
                "source_game": "fo3",
                "source_plugin": "Fallout3.esm",
                "source_form_key": f"{form_id + 0x3000:08X}@Fallout3.esm",
                "contributing_plugin": "ThePitt.esm",
                "source_parent_form_key": None,
                "merged_parent_form_key": None,
                "child_group_type": None,
            }
            for signature, form_ids in (
                list(slice_records.items())
                + [("NPC_", [0x123193, 0x1300F0])]
            )
            for form_id in form_ids
        ],
        "fnv_force_greet_donor_form_key": "05B307@Fallout4.esm",
        "legacy_pack_origins": _legacy_pack_origins(),
        "legacy_pack_raw_source_counts": {"fnv": 4_888, "fo3": 4_567},
        "legacy_pack_expected_counts": {"fnv": 4, "fo3": 0},
        "legacy_pack_provenance_required": True,
    }
    with ConversionRun.create_new(
        "fnv",
        "fo4",
        str(source_path),
        target_path.name,
        master_plugin_paths=[str(donor_path)],
        config=config,
    ) as run:
        native = load_native_module()
        translated = run.run_phase("translate_v2", mod_path="", params={})
        pre_tail_path = tmp_path / "FNVQuestVerticalSlicePreTail.esm"
        run.save_target(
            str(pre_tail_path),
            run_nvnm_validator=False,
            preserve_target_identity=True,
        )
        with Plugin.load(pre_tail_path, game="fo4") as pre_tail:
            pre_tail_export = export_data(pre_tail)
        _assert_selected_achrs_are_persistent_cell_children(pre_tail_export["items"])
        legacy = native.conversion_run_fnv_legacy_scripting_from_run(
            run.id,
            OUTPUT_PREFIX,
            source_path.name,
            str(tmp_path),
        )
        run.save_target(str(target_path), run_nvnm_validator=False)

    assert translated["records_changed"] >= 1
    with Plugin.load(target_path, game="fo4") as target:
        target_export = export_data(target)

    assert legacy["translated_scripts"] == 4, legacy
    assert legacy["translated_quests"] == 2, legacy
    assert legacy["translated_infos"] == 5, legacy
    assert legacy["translated_scenes"] == 0, legacy
    assert legacy["records_failed"] == 0, legacy
    component_receipts = legacy["quest_runtime_components"]
    assert len(component_receipts) == 2
    assert all(
        component["provenance"]["game"] == "fo3"
        and component["provenance"]["source_plugin"] == "Fallout3.esm"
        and component["provenance"]["graft"]["source_form_key"]
        != component["provenance"]["graft"]["graft_form_key"]
        for component in component_receipts
    )
    power_armor_component = next(
        component
        for component in component_receipts
        if component["source_root"]["form_key"].startswith(
            f"{FREEFORM_POWER_ARMOR_QUEST_ID:06X}"
        )
    )
    assert power_armor_component["provenance"]["graft"]["graft_form_key"].replace(
        "@", ":"
    ) == power_armor_component["source_root"]["form_key"].replace("@", ":")
    assert power_armor_component["target_root"] == {
        "signature": "QUST",
        "form_key": f"{FREEFORM_POWER_ARMOR_QUEST_ID:06X}:FalloutNV.esm",
    }
    assert power_armor_component["start_disposition"] == {"kind": "autostart"}
    assert len(power_armor_component["start_routes"]) == 1
    start_route = power_armor_component["start_routes"][0]
    assert start_route["quest"] == power_armor_component["target_root"]
    assert start_route["producer_evidence_id"].startswith("autostart:")
    assert start_route["node_chain"] == []
    greeting_voice_entries = [
        entry
        for entry in legacy["voice_manifest"]["entries"]
        if entry["info_form_id"] in {f"{form_id:08X}" for form_id in GREETING_INFO_IDS}
    ]
    assert [entry["info_form_id"] for entry in greeting_voice_entries] == [
        f"{form_id:08X}" for form_id in GREETING_INFO_IDS
    ]
    assert [
        Path(entry["source_candidates"][0]).name.lower()
        for entry in greeting_voice_entries
    ] == [f"{form_id:08x}_1.ogg" for form_id in GREETING_INFO_IDS]
    generated_psc_classes = {
        generated["class_name"] for generated in legacy["generated_psc_classes"]
    }
    assert generated_psc_classes == {
        f"{OUTPUT_PREFIX}_S_11FC64",
        f"{OUTPUT_PREFIX}_S_123191",
        f"{OUTPUT_PREFIX}_S_134491",
        f"{OUTPUT_PREFIX}_S_166305",
        f"QF_{OUTPUT_PREFIX}_06136D",
        f"QF_{OUTPUT_PREFIX}_11F935",
        "TIF__130161",
        "TIF__134B9B",
        f"{OUTPUT_PREFIX}_FnvSliceCompat",
    }
    assert all(
        class_name.startswith((f"{OUTPUT_PREFIX}_", f"QF_{OUTPUT_PREFIX}_", "TIF__"))
        for class_name in generated_psc_classes
    )
    assert all(len(class_name) <= 38 for class_name in generated_psc_classes)
    generated_psc_sources = {
        generated["class_name"]: (
            tmp_path / generated["relative_source_path"]
        ).read_text(encoding="utf-8")
        for generated in legacy["generated_psc_classes"]
    }
    for source in generated_psc_sources.values():
        property_names = _psc_property_names(source)
        assert len(property_names) == len(
            {property_name.casefold() for property_name in property_names}
        )
    hostage_script_text = generated_psc_sources[f"{OUTPUT_PREFIX}_S_123191"]
    assert (
        _psc_property_names(hostage_script_text).count("TecMineHostageFreedGreeting")
        == 1
    )

    target_items = target_export["items"]
    _assert_exact_audited_output_closure(target_items, slice_records)
    target_quest_group = _top_group_by_signature(target_items, "QUST")
    target_quest = _record_by_id(target_quest_group["children"], "QUST", QUEST_ID)
    target_power_armor_quest = _record_by_id(
        target_quest_group["children"], "QUST", FREEFORM_POWER_ARMOR_QUEST_ID
    )
    reported_target_form_key = power_armor_component["target_root"]["form_key"]
    reported_target_local = reported_target_form_key.replace("@", ":").split(":", 1)[0]
    assert target_power_armor_quest["form_id"] == reported_target_local
    target_power_armor_header = (
        _subrecord_data(target_power_armor_quest, "DNAM")
        or _subrecord_data(target_power_armor_quest, "DATA")
    )
    assert target_power_armor_header
    assert int.from_bytes(bytes.fromhex(target_power_armor_header[0][:4]), "little") & 1
    target_quest_children = next(
        child
        for child in target_quest_group["children"]
        if child.get("type") == "group"
        and child.get("group_type") == 10
        and _form_id_local(child["label_hex"]) == QUEST_ID
    )
    assert target_quest["form_id"] == f"{QUEST_ID:06X}"
    for topic_id, editor_id in ORDINARY_TOPIC_EDITOR_IDS.items():
        target_topic = _record_by_id(
            target_quest_children["children"], "DIAL", topic_id
        )
        assert target_topic["editor_id"] == editor_id
    target_greeting_topics = [
        topic
        for topic in _records(target_quest_children["children"], "DIAL")
        if int(topic["form_id"], 16)
        not in {FIRST_TOPIC_ID, SECOND_TOPIC_ID, EMPTY_TOPIC_ID}
    ]
    assert len(target_greeting_topics) == 1
    target_greeting_topic = target_greeting_topics[0]
    assert target_greeting_topic["form_id"] != f"{GREETING_TOPIC_ID:06X}"
    assert target_greeting_topic["editor_id"].startswith(f"{OUTPUT_PREFIX}_")
    assert _subrecord_form_id_locals(target_greeting_topic, "QNAM") == [QUEST_ID]
    assert _subrecord_data(target_greeting_topic, "TIFC") == [_hex_u32(3)]
    target_greeting_keywords = _records(target_items, "KYWD")
    assert len(target_greeting_keywords) == 1
    target_greeting_keyword = target_greeting_keywords[0]
    assert target_greeting_keyword["editor_id"].startswith(f"{OUTPUT_PREFIX}_")
    assert _subrecord_form_id_locals(target_greeting_topic, "KNAM") == [
        int(target_greeting_keyword["form_id"], 16)
    ]
    target_greeting_topic_id = int(target_greeting_topic["form_id"], 16)
    target_greeting_infos = _topic_info_group(
        target_greeting_topic_id, target_quest_children
    )
    assert [
        record["form_id"]
        for record in _records(target_greeting_infos["children"], "INFO")
    ] == [f"{form_id:06X}" for form_id in GREETING_INFO_IDS]
    assert _record_by_id(
        _topic_info_group(FIRST_TOPIC_ID, target_quest_children)["children"],
        "INFO",
        FIRST_INFO_ID,
    )
    assert _record_by_id(
        _topic_info_group(SECOND_TOPIC_ID, target_quest_children)["children"],
        "INFO",
        SECOND_INFO_ID,
    )
    assert not any(
        child.get("type") == "group"
        and child.get("group_type") == 7
        and _form_id_local(child["label_hex"]) == EMPTY_TOPIC_ID
        for child in target_quest_children["children"]
    )
    synthesized_branch = _record_by_editor_id(
        target_quest_children["children"],
        "DLBR",
        f"{OUTPUT_PREFIX}_VTechatticupDialogueBranch",
    )
    synthesized_scene = _record_by_editor_id(
        target_quest_children["children"],
        "SCEN",
        f"{OUTPUT_PREFIX}_VTechatticupDialogueScene",
    )
    assert _subrecord_form_id_locals(synthesized_branch, "QNAM") == [QUEST_ID]
    assert _subrecord_form_id_locals(synthesized_branch, "SNAM") == [SECOND_TOPIC_ID]
    assert _subrecord_form_id_locals(synthesized_scene, "PNAM") == [QUEST_ID]
    assert target_greeting_topic_id not in [
        _form_id_local(data_hex)
        for field in synthesized_scene["subrecords"]
        for data_hex in [field["data_hex"]]
        if len(data_hex) >= 8
    ]
    assert (
        "Keyword Property TecMineHostageFreedGreeting Auto Const" in hostage_script_text
    )
    assert "SayCustom(TecMineHostageFreedGreeting" in hostage_script_text
    assert "Topic Property" not in hostage_script_text

    for form_id in (HOSTAGE_ONE_ID, HOSTAGE_TWO_ID, RENOLDS_ID):
        assert _record_by_id(target_items, "ACHR", form_id)["flags"] == "00000400"
    _assert_selected_achrs_are_persistent_cell_children(target_items)
    target_trigger = _record_by_id(target_items, "REFR", TRIGGER_ID)
    assert target_trigger["flags"] == "00000400"
    assert target_trigger["editor_id"] == "TecRenoldsDialogueActREF"
    assert _subrecord_form_id_locals(target_trigger, "NAME") == [TRIGGER_BASE_ID]
    assert _subrecord_data(target_trigger, "XPRM") == [
        "00001C440000814400003043CDCC4C3F9A99993ECDCC4C3E9A99193E01000000"
    ]
    assert _subrecord_data(target_trigger, "XMBO") == ["00001C440000814400003043"]
    assert _subrecord_data(target_trigger, "DATA") == [
        "00905647002039C700C09445000000000000000023861840"
    ]
    target_trigger_base = _record_by_id(target_items, "ACTI", TRIGGER_BASE_ID)
    assert target_trigger_base["editor_id"] == "TecRenoldsDialogueActivator"
    assert [
        int(record["form_id"], 16)
        for record in _records(target_items, "REFR")
        if _subrecord_form_id_locals(record, "NAME") == [TRIGGER_BASE_ID]
    ] == [TRIGGER_ID]
    target_renolds = _record_by_id(target_items, "NPC_", RENOLDS_BASE_ID)
    assert _subrecord_form_id_locals(target_renolds, "PKID") == [0x133F3E]
    assert _record_by_id(target_items, "MGEF", MAGIC_EFFECT_ID)["form_id"] == "0CB05D"
    target_spell = _record_by_id(target_items, "SPEL", SPELL_ID)
    assert target_spell["form_id"] == f"{SPELL_ID:06X}"
    assert _subrecord_data(target_spell, "EFID") == [
        _hex_u32(effect_id) for effect_id in FO4_RESTORE_CONDITION_EFFECT_IDS
    ]
    assert len(_subrecord_data(target_spell, "EFIT")) == 6
    assert _record_by_id(target_items, "PERK", PERK_ID)["form_id"] == f"{PERK_ID:06X}"
    for form_id, editor_id in PACK_EDITOR_IDS.items():
        target_pack = _record_by_id(target_items, "PACK", form_id)
        assert target_pack["form_id"] == f"{form_id:06X}"
        assert target_pack["editor_id"] == editor_id
