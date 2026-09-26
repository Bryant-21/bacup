use esp_authoring_core::plugin_runtime::build_vmad_bytes_from_payload;
use rustc_hash::FxHashSet;

use crate::fixups::quest_script_vmad::{AttachResult, attach_script_bytes};
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::session::PluginSession;
use crate::sym::StringInterner;

const MATERIALIZER_SCRIPT_NAME: &str = "B21:LocalEncounterMaterializer";
const VMAD_VERSION: u16 = 6;
const VMAD_OBJECT_FORMAT: u16 = 2;
const VMAD_PROPERTY_FLAG_EDITED: u8 = 1;

pub struct MaterializeFo76LocalEncounterWavesFixup;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CandidateSpec {
    source_actor: u32,
    spawn_slot: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceWaveContract {
    source_wave: u32,
    source_wave_eid: &'static str,
    candidates: &'static [CandidateSpec],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WaveSpec {
    source_wave: u32,
    source_wave_eid: &'static str,
    source_wave_contracts: &'static [SourceWaveContract],
    collection_alias: i32,
    spawn_marker_alias: i32,
    preparation_stage: i32,
    stage_after_preparation: i32,
    replace_collection: bool,
    actor_count: i32,
    spawn_slot_count: i32,
    start_stage: i32,
    end_stage: i32,
    candidates: &'static [CandidateSpec],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct QuestSpec {
    source_quest: u32,
    source_quest_eid: &'static str,
    waves: &'static [WaveSpec],
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TargetWaveSpec {
    collection_alias: i32,
    spawn_marker_alias: i32,
    preparation_stage: i32,
    stage_after_preparation: i32,
    replace_collection: bool,
    actor_count: i32,
    spawn_slot_count: i32,
    candidates: Vec<(FormKey, i32)>,
}

const MQ003_WAVE0_CANDIDATES: &[CandidateSpec] = &[
    CandidateSpec {
        source_actor: 0x08E624,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x31B1FF,
        spawn_slot: 1,
    },
    CandidateSpec {
        source_actor: 0x08E624,
        spawn_slot: 2,
    },
    CandidateSpec {
        source_actor: 0x31B1FF,
        spawn_slot: 2,
    },
    CandidateSpec {
        source_actor: 0x31B1FF,
        spawn_slot: 3,
    },
];
const MQ003_WAVE1_CANDIDATES: &[CandidateSpec] = &[
    CandidateSpec {
        source_actor: 0x59BBDA,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x31B1FF,
        spawn_slot: 1,
    },
    CandidateSpec {
        source_actor: 0x31B1FF,
        spawn_slot: 2,
    },
];
const MQ003_WAVES: &[WaveSpec] = &[
    WaveSpec {
        source_wave: 0x529DF0,
        source_wave_eid: "WaveTypeScorched",
        source_wave_contracts: &[],
        collection_alias: 17,
        spawn_marker_alias: 43,
        preparation_stage: -1,
        stage_after_preparation: -1,
        replace_collection: false,
        actor_count: 4,
        spawn_slot_count: 4,
        start_stage: 710,
        end_stage: 715,
        candidates: MQ003_WAVE0_CANDIDATES,
    },
    WaveSpec {
        source_wave: 0x59BBDD,
        source_wave_eid: "W05_MQ_003P_Muscle_ScorchedBoss",
        source_wave_contracts: &[],
        collection_alias: 17,
        spawn_marker_alias: 69,
        preparation_stage: 715,
        stage_after_preparation: 725,
        replace_collection: true,
        actor_count: 3,
        spawn_slot_count: 3,
        start_stage: 725,
        end_stage: 800,
        candidates: MQ003_WAVE1_CANDIDATES,
    },
];

const MQR203_WAVE0_CANDIDATES: &[CandidateSpec] = &[CandidateSpec {
    source_actor: 0x075337,
    spawn_slot: 0,
}];
const MQR203_WAVE1_CANDIDATES: &[CandidateSpec] = &[
    CandidateSpec {
        source_actor: 0x572552,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x572359,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x572353,
        spawn_slot: 0,
    },
];
const MQR203_WAVES: &[WaveSpec] = &[
    WaveSpec {
        source_wave: 0x4F60B4,
        source_wave_eid: "WaveTypeGhouls",
        source_wave_contracts: &[],
        collection_alias: 15,
        spawn_marker_alias: 24,
        preparation_stage: -1,
        stage_after_preparation: -1,
        replace_collection: false,
        actor_count: 1,
        spawn_slot_count: 1,
        start_stage: 800,
        end_stage: 810,
        candidates: MQR203_WAVE0_CANDIDATES,
    },
    WaveSpec {
        source_wave: 0x59253F,
        source_wave_eid: "W05_MQR_203P_WaveType_Floaters",
        source_wave_contracts: &[],
        collection_alias: 16,
        spawn_marker_alias: 24,
        preparation_stage: -1,
        stage_after_preparation: -1,
        replace_collection: false,
        actor_count: 5,
        spawn_slot_count: 1,
        start_stage: 1200,
        end_stage: 1210,
        candidates: MQR203_WAVE1_CANDIDATES,
    },
];

const MQS203_WAVE0_CANDIDATES: &[CandidateSpec] = &[
    CandidateSpec {
        source_actor: 0x0EF3F2,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x0E9293,
        spawn_slot: 1,
    },
    CandidateSpec {
        source_actor: 0x43E946,
        spawn_slot: 2,
    },
    CandidateSpec {
        source_actor: 0x43E946,
        spawn_slot: 3,
    },
    CandidateSpec {
        source_actor: 0x186E01,
        spawn_slot: 4,
    },
];
const MQS203_TOOL_WAVE_CANDIDATES: &[CandidateSpec] = &[
    CandidateSpec {
        source_actor: 0x186E01,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x186E01,
        spawn_slot: 1,
    },
    CandidateSpec {
        source_actor: 0x43E946,
        spawn_slot: 1,
    },
    CandidateSpec {
        source_actor: 0x10EAE6,
        spawn_slot: 2,
    },
];
const MQS203_WAVES: &[WaveSpec] = &[
    WaveSpec {
        source_wave: 0x59DA1A,
        source_wave_eid: "W05_MQS_203PWaveTypeProtectron",
        source_wave_contracts: &[],
        collection_alias: 120,
        spawn_marker_alias: 108,
        preparation_stage: -1,
        stage_after_preparation: -1,
        replace_collection: false,
        actor_count: 5,
        spawn_slot_count: 5,
        start_stage: 1001,
        end_stage: -1,
        candidates: MQS203_WAVE0_CANDIDATES,
    },
    WaveSpec {
        source_wave: 0x5A1C76,
        source_wave_eid: "W05_MQS_203P_ToolWaveRobots",
        source_wave_contracts: &[],
        collection_alias: 121,
        spawn_marker_alias: 109,
        preparation_stage: -1,
        stage_after_preparation: -1,
        replace_collection: false,
        actor_count: 3,
        spawn_slot_count: 3,
        start_stage: 2000,
        end_stage: 2050,
        candidates: MQS203_TOOL_WAVE_CANDIDATES,
    },
    WaveSpec {
        source_wave: 0x5A1C76,
        source_wave_eid: "W05_MQS_203P_ToolWaveRobots",
        source_wave_contracts: &[],
        collection_alias: 121,
        spawn_marker_alias: 109,
        preparation_stage: 2050,
        stage_after_preparation: -1,
        replace_collection: true,
        actor_count: 3,
        spawn_slot_count: 3,
        start_stage: 2100,
        end_stage: 2150,
        candidates: MQS203_TOOL_WAVE_CANDIDATES,
    },
];

const ASTRONAUT_WAVE_CANDIDATES: &[CandidateSpec] = &[
    CandidateSpec {
        source_actor: 0x075335,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x075335,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x14AE58,
        spawn_slot: 0,
    },
];
const ASTRONAUT_SOURCE_WAVES: &[SourceWaveContract] = &[SourceWaveContract {
    source_wave: 0x5A1246,
    source_wave_eid: "WaveType_COMP_Astronaut_SuperMutant",
    candidates: ASTRONAUT_WAVE_CANDIDATES,
}];
const ASTRONAUT_WAVES: &[WaveSpec] = &[WaveSpec {
    source_wave: 0x5A1246,
    source_wave_eid: "WaveType_COMP_Astronaut_SuperMutant",
    source_wave_contracts: ASTRONAUT_SOURCE_WAVES,
    collection_alias: 2,
    spawn_marker_alias: 0,
    preparation_stage: -1,
    stage_after_preparation: -1,
    replace_collection: false,
    actor_count: 3,
    spawn_slot_count: 1,
    start_stage: 10,
    end_stage: 9000,
    candidates: ASTRONAUT_WAVE_CANDIDATES,
}];

const BECKETT_NARROW_ESCAPE_WAVE_CANDIDATES: &[CandidateSpec] = &[
    CandidateSpec {
        source_actor: 0x53C52F,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x53C531,
        spawn_slot: 1,
    },
    CandidateSpec {
        source_actor: 0x588087,
        spawn_slot: 1,
    },
    CandidateSpec {
        source_actor: 0x53C52F,
        spawn_slot: 2,
    },
    CandidateSpec {
        source_actor: 0x53C531,
        spawn_slot: 2,
    },
    CandidateSpec {
        source_actor: 0x53C531,
        spawn_slot: 3,
    },
];
const BECKETT_NARROW_ESCAPE_SOURCE_WAVES: &[SourceWaveContract] = &[SourceWaveContract {
    source_wave: 0x55DE43,
    source_wave_eid: "WaveTypeBloodEagle",
    candidates: BECKETT_NARROW_ESCAPE_WAVE_CANDIDATES,
}];
const BECKETT_NARROW_ESCAPE_WAVES: &[WaveSpec] = &[WaveSpec {
    source_wave: 0x55DE43,
    source_wave_eid: "WaveTypeBloodEagle",
    source_wave_contracts: BECKETT_NARROW_ESCAPE_SOURCE_WAVES,
    collection_alias: 24,
    spawn_marker_alias: 27,
    preparation_stage: 595,
    stage_after_preparation: -1,
    replace_collection: false,
    actor_count: 8,
    spawn_slot_count: 4,
    start_stage: 595,
    end_stage: -1,
    candidates: BECKETT_NARROW_ESCAPE_WAVE_CANDIDATES,
}];

const BURN_GRUNT_HEAVY_CANDIDATES: &[CandidateSpec] = &[
    CandidateSpec {
        source_actor: 0x7D1086,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x7D107F,
        spawn_slot: 0,
    },
];
const BURN_GRUNT_HEAVY_AUTO_CANDIDATES: &[CandidateSpec] = &[
    CandidateSpec {
        source_actor: 0x7D1085,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x7D107E,
        spawn_slot: 0,
    },
];
const BURN_GRUNT_MELEE_CANDIDATES: &[CandidateSpec] = &[
    CandidateSpec {
        source_actor: 0x7D1084,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x7D107C,
        spawn_slot: 0,
    },
];
const BURN_GRUNT_PISTOL_CANDIDATES: &[CandidateSpec] = &[
    CandidateSpec {
        source_actor: 0x7D1083,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x7D107D,
        spawn_slot: 0,
    },
];
const BURN_GRUNT_RIFLE_AUTO_CANDIDATES: &[CandidateSpec] = &[
    CandidateSpec {
        source_actor: 0x7D1082,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x7D107B,
        spawn_slot: 0,
    },
];
const BURN_GRUNT_SHOTGUN_CANDIDATES: &[CandidateSpec] = &[
    CandidateSpec {
        source_actor: 0x7D1081,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x7D107A,
        spawn_slot: 0,
    },
];
const BURN_GRUNT_SNIPER_CANDIDATES: &[CandidateSpec] = &[
    CandidateSpec {
        source_actor: 0x7D1080,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x7D1079,
        spawn_slot: 0,
    },
];
const BURN_GRUNT_TARGET_CANDIDATES: &[CandidateSpec] = &[
    BURN_GRUNT_HEAVY_CANDIDATES[0],
    BURN_GRUNT_HEAVY_CANDIDATES[1],
    BURN_GRUNT_HEAVY_AUTO_CANDIDATES[0],
    BURN_GRUNT_HEAVY_AUTO_CANDIDATES[1],
    BURN_GRUNT_MELEE_CANDIDATES[0],
    BURN_GRUNT_MELEE_CANDIDATES[1],
    BURN_GRUNT_PISTOL_CANDIDATES[0],
    BURN_GRUNT_PISTOL_CANDIDATES[1],
    BURN_GRUNT_RIFLE_AUTO_CANDIDATES[0],
    BURN_GRUNT_RIFLE_AUTO_CANDIDATES[1],
    BURN_GRUNT_SHOTGUN_CANDIDATES[0],
    BURN_GRUNT_SHOTGUN_CANDIDATES[1],
    BURN_GRUNT_SNIPER_CANDIDATES[0],
    BURN_GRUNT_SNIPER_CANDIDATES[1],
];
const BURN_GRUNT_SOURCE_WAVES: &[SourceWaveContract] = &[
    SourceWaveContract {
        source_wave: 0x833A1B,
        source_wave_eid: "Burn_Bounty_Grunt_Heavy",
        candidates: BURN_GRUNT_HEAVY_CANDIDATES,
    },
    SourceWaveContract {
        source_wave: 0x833A1A,
        source_wave_eid: "Burn_Bounty_Grunt_HeavyAuto",
        candidates: BURN_GRUNT_HEAVY_AUTO_CANDIDATES,
    },
    SourceWaveContract {
        source_wave: 0x833A1E,
        source_wave_eid: "Burn_Bounty_Grunt_Melee",
        candidates: BURN_GRUNT_MELEE_CANDIDATES,
    },
    SourceWaveContract {
        source_wave: 0x833A19,
        source_wave_eid: "Burn_Bounty_Grunt_Pistol",
        candidates: BURN_GRUNT_PISTOL_CANDIDATES,
    },
    SourceWaveContract {
        source_wave: 0x833A1C,
        source_wave_eid: "Burn_Bounty_Grunt_RifleAuto",
        candidates: BURN_GRUNT_RIFLE_AUTO_CANDIDATES,
    },
    SourceWaveContract {
        source_wave: 0x833A1D,
        source_wave_eid: "Burn_Bounty_Grunt_Shotgun",
        candidates: BURN_GRUNT_SHOTGUN_CANDIDATES,
    },
    SourceWaveContract {
        source_wave: 0x833A1F,
        source_wave_eid: "Burn_Bounty_Grunt_Sniper",
        candidates: BURN_GRUNT_SNIPER_CANDIDATES,
    },
];
const BURN_GRUNT_WAVES: &[WaveSpec] = &[WaveSpec {
    source_wave: 0x833A1B,
    source_wave_eid: "Burn_Bounty_Grunt_Heavy",
    source_wave_contracts: BURN_GRUNT_SOURCE_WAVES,
    collection_alias: 17,
    spawn_marker_alias: 3,
    preparation_stage: -1,
    stage_after_preparation: -1,
    replace_collection: false,
    actor_count: 1,
    spawn_slot_count: 1,
    start_stage: 200,
    end_stage: 300,
    candidates: BURN_GRUNT_TARGET_CANDIDATES,
}];

const EN05_NORMAL_SOURCE: &[CandidateSpec] = &[
    CandidateSpec {
        source_actor: 0x15E058,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x15E058,
        spawn_slot: 1,
    },
    CandidateSpec {
        source_actor: 0x440C80,
        spawn_slot: 2,
    },
    CandidateSpec {
        source_actor: 0x15E058,
        spawn_slot: 3,
    },
    CandidateSpec {
        source_actor: 0x15E058,
        spawn_slot: 4,
    },
    CandidateSpec {
        source_actor: 0x440C80,
        spawn_slot: 5,
    },
    CandidateSpec {
        source_actor: 0x15E058,
        spawn_slot: 6,
    },
    CandidateSpec {
        source_actor: 0x15E058,
        spawn_slot: 7,
    },
    CandidateSpec {
        source_actor: 0x440C80,
        spawn_slot: 8,
    },
    CandidateSpec {
        source_actor: 0x15E058,
        spawn_slot: 9,
    },
    CandidateSpec {
        source_actor: 0x15E058,
        spawn_slot: 10,
    },
    CandidateSpec {
        source_actor: 0x440C80,
        spawn_slot: 11,
    },
];
const EN05_NORMAL_CONTRACT: &[SourceWaveContract] = &[SourceWaveContract {
    source_wave: 0x4F60F3,
    source_wave_eid: "WaveType_SpecialEN05CombatExercise",
    candidates: EN05_NORMAL_SOURCE,
}];
const EN05_BOSS_SOURCE: &[CandidateSpec] = &[
    CandidateSpec {
        source_actor: 0x43C423,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x15E058,
        spawn_slot: 1,
    },
    CandidateSpec {
        source_actor: 0x15E058,
        spawn_slot: 2,
    },
    CandidateSpec {
        source_actor: 0x440C80,
        spawn_slot: 3,
    },
    CandidateSpec {
        source_actor: 0x15E058,
        spawn_slot: 4,
    },
    CandidateSpec {
        source_actor: 0x15E058,
        spawn_slot: 5,
    },
    CandidateSpec {
        source_actor: 0x440C80,
        spawn_slot: 6,
    },
    CandidateSpec {
        source_actor: 0x15E058,
        spawn_slot: 7,
    },
    CandidateSpec {
        source_actor: 0x15E058,
        spawn_slot: 8,
    },
    CandidateSpec {
        source_actor: 0x440C80,
        spawn_slot: 9,
    },
    CandidateSpec {
        source_actor: 0x15E058,
        spawn_slot: 10,
    },
    CandidateSpec {
        source_actor: 0x15E058,
        spawn_slot: 11,
    },
];
const EN05_BOSS_CONTRACT: &[SourceWaveContract] = &[SourceWaveContract {
    source_wave: 0x45311B,
    source_wave_eid: "WaveType_SpecialEN05CombatExerciseBoss",
    candidates: EN05_BOSS_SOURCE,
}];
const EN05_LACKEYS: &[CandidateSpec] = &[
    CandidateSpec {
        source_actor: 0x15E058,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x15E058,
        spawn_slot: 1,
    },
];
const EN05_COMBAT_WAVES: &[WaveSpec] = &[
    WaveSpec {
        source_wave: 0x4F60F3,
        source_wave_eid: "WaveType_SpecialEN05CombatExercise",
        source_wave_contracts: EN05_NORMAL_CONTRACT,
        collection_alias: 6,
        spawn_marker_alias: 8,
        preparation_stage: 20,
        stage_after_preparation: -1,
        replace_collection: false,
        actor_count: 3,
        spawn_slot_count: 3,
        start_stage: 20,
        end_stage: 30,
        candidates: EN05_NORMAL_SOURCE,
    },
    WaveSpec {
        source_wave: 0x4F60F3,
        source_wave_eid: "WaveType_SpecialEN05CombatExercise",
        source_wave_contracts: EN05_NORMAL_CONTRACT,
        collection_alias: 6,
        spawn_marker_alias: 8,
        preparation_stage: 32,
        stage_after_preparation: -1,
        replace_collection: true,
        actor_count: 3,
        spawn_slot_count: 3,
        start_stage: 32,
        end_stage: 40,
        candidates: EN05_NORMAL_SOURCE,
    },
    WaveSpec {
        source_wave: 0x45311B,
        source_wave_eid: "WaveType_SpecialEN05CombatExerciseBoss",
        source_wave_contracts: EN05_BOSS_CONTRACT,
        collection_alias: 13,
        spawn_marker_alias: 8,
        preparation_stage: 42,
        stage_after_preparation: -1,
        replace_collection: false,
        actor_count: 1,
        spawn_slot_count: 1,
        start_stage: 42,
        end_stage: 100,
        candidates: EN05_BOSS_SOURCE,
    },
    WaveSpec {
        source_wave: 0x45311B,
        source_wave_eid: "WaveType_SpecialEN05CombatExerciseBoss",
        source_wave_contracts: EN05_BOSS_CONTRACT,
        collection_alias: 6,
        spawn_marker_alias: 8,
        preparation_stage: 42,
        stage_after_preparation: -1,
        replace_collection: true,
        actor_count: 2,
        spawn_slot_count: 2,
        start_stage: 42,
        end_stage: 100,
        candidates: EN05_LACKEYS,
    },
];

const MTNS01_SOURCE_CANDIDATES: &[CandidateSpec] = &[
    CandidateSpec {
        source_actor: 0x075341,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x117D85,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x088F14,
        spawn_slot: 1,
    },
    CandidateSpec {
        source_actor: 0x0948B3,
        spawn_slot: 1,
    },
    CandidateSpec {
        source_actor: 0x075335,
        spawn_slot: 2,
    },
    CandidateSpec {
        source_actor: 0x075335,
        spawn_slot: 3,
    },
    CandidateSpec {
        source_actor: 0x088F14,
        spawn_slot: 4,
    },
    CandidateSpec {
        source_actor: 0x0948B3,
        spawn_slot: 4,
    },
    CandidateSpec {
        source_actor: 0x075335,
        spawn_slot: 5,
    },
    CandidateSpec {
        source_actor: 0x075335,
        spawn_slot: 6,
    },
    CandidateSpec {
        source_actor: 0x088F14,
        spawn_slot: 7,
    },
    CandidateSpec {
        source_actor: 0x0948B3,
        spawn_slot: 7,
    },
    CandidateSpec {
        source_actor: 0x075335,
        spawn_slot: 8,
    },
    CandidateSpec {
        source_actor: 0x075335,
        spawn_slot: 9,
    },
    CandidateSpec {
        source_actor: 0x088F14,
        spawn_slot: 10,
    },
    CandidateSpec {
        source_actor: 0x0948B3,
        spawn_slot: 10,
    },
    CandidateSpec {
        source_actor: 0x075335,
        spawn_slot: 11,
    },
];
// Three occupied source slots are the bounded single-player substitute for server difficulty scaling.
const MTNS01_WAVES: &[WaveSpec] = &[WaveSpec {
    source_wave: 0x43D199,
    source_wave_eid: "WaveTypeSuperMutantBoss",
    source_wave_contracts: &[SourceWaveContract {
        source_wave: 0x43D199,
        source_wave_eid: "WaveTypeSuperMutantBoss",
        candidates: MTNS01_SOURCE_CANDIDATES,
    }],
    collection_alias: 101,
    spawn_marker_alias: 86,
    preparation_stage: 401,
    stage_after_preparation: -1,
    replace_collection: false,
    actor_count: 3,
    spawn_slot_count: 3,
    start_stage: 401,
    end_stage: 420,
    candidates: MTNS01_SOURCE_CANDIDATES,
}];

const FS02_SOURCE_CANDIDATES: &[CandidateSpec] = &[
    CandidateSpec {
        source_actor: 0x075335,
        spawn_slot: 0,
    },
    CandidateSpec {
        source_actor: 0x088F14,
        spawn_slot: 1,
    },
    CandidateSpec {
        source_actor: 0x0948B3,
        spawn_slot: 1,
    },
    CandidateSpec {
        source_actor: 0x075335,
        spawn_slot: 2,
    },
    CandidateSpec {
        source_actor: 0x075335,
        spawn_slot: 3,
    },
    CandidateSpec {
        source_actor: 0x088F14,
        spawn_slot: 4,
    },
    CandidateSpec {
        source_actor: 0x0948B3,
        spawn_slot: 4,
    },
    CandidateSpec {
        source_actor: 0x075335,
        spawn_slot: 5,
    },
    CandidateSpec {
        source_actor: 0x075335,
        spawn_slot: 6,
    },
    CandidateSpec {
        source_actor: 0x088F14,
        spawn_slot: 7,
    },
    CandidateSpec {
        source_actor: 0x0948B3,
        spawn_slot: 7,
    },
    CandidateSpec {
        source_actor: 0x075335,
        spawn_slot: 8,
    },
    CandidateSpec {
        source_actor: 0x075335,
        spawn_slot: 9,
    },
    CandidateSpec {
        source_actor: 0x088F14,
        spawn_slot: 10,
    },
    CandidateSpec {
        source_actor: 0x0948B3,
        spawn_slot: 10,
    },
    CandidateSpec {
        source_actor: 0x075335,
        spawn_slot: 11,
    },
];
// Three occupied source slots are the bounded single-player substitute for server difficulty scaling.
const FS02_WAVES: &[WaveSpec] = &[WaveSpec {
    source_wave: 0x4F60F1,
    source_wave_eid: "WaveTypeSuperMutant",
    source_wave_contracts: &[SourceWaveContract {
        source_wave: 0x4F60F1,
        source_wave_eid: "WaveTypeSuperMutant",
        candidates: FS02_SOURCE_CANDIDATES,
    }],
    collection_alias: 22,
    spawn_marker_alias: 25,
    preparation_stage: 205,
    stage_after_preparation: -1,
    replace_collection: false,
    actor_count: 3,
    spawn_slot_count: 3,
    start_stage: 205,
    end_stage: 260,
    candidates: FS02_SOURCE_CANDIDATES,
}];

const MTNL01_GLOWING_ONE: &[CandidateSpec] = &[CandidateSpec {
    source_actor: 0x48F2AC,
    spawn_slot: 0,
}];
const MTNL01_WENDIGO: &[CandidateSpec] = &[CandidateSpec {
    source_actor: 0x48F2AD,
    spawn_slot: 0,
}];
const MTNL01_DAVID: &[CandidateSpec] = &[CandidateSpec {
    source_actor: 0x4EA3FB,
    spawn_slot: 0,
}];

const MTNL01_WAVES: &[WaveSpec] = &[
    WaveSpec {
        source_wave: 0x43C0C8,
        source_wave_eid: "WaveType_SpecialMTNL01GlowingOne",
        source_wave_contracts: &[SourceWaveContract {
            source_wave: 0x43C0C8,
            source_wave_eid: "WaveType_SpecialMTNL01GlowingOne",
            candidates: MTNL01_GLOWING_ONE,
        }],
        collection_alias: 25,
        spawn_marker_alias: 40,
        preparation_stage: -1,
        stage_after_preparation: -1,
        replace_collection: false,
        actor_count: 1,
        spawn_slot_count: 1,
        start_stage: 211,
        end_stage: -1,
        candidates: MTNL01_GLOWING_ONE,
    },
    WaveSpec {
        source_wave: 0x43C0CC,
        source_wave_eid: "WaveType_SpecialMTNL01ProgenitorWendigoBoss",
        source_wave_contracts: &[SourceWaveContract {
            source_wave: 0x43C0CC,
            source_wave_eid: "WaveType_SpecialMTNL01ProgenitorWendigoBoss",
            candidates: MTNL01_WENDIGO,
        }],
        collection_alias: 26,
        spawn_marker_alias: 41,
        preparation_stage: -1,
        stage_after_preparation: -1,
        replace_collection: false,
        actor_count: 1,
        spawn_slot_count: 1,
        start_stage: 511,
        end_stage: -1,
        candidates: MTNL01_WENDIGO,
    },
    WaveSpec {
        source_wave: 0x43C0CD,
        source_wave_eid: "WaveType_SpecialMTNL01DavidScorchedBoss",
        source_wave_contracts: &[SourceWaveContract {
            source_wave: 0x43C0CD,
            source_wave_eid: "WaveType_SpecialMTNL01DavidScorchedBoss",
            candidates: MTNL01_DAVID,
        }],
        collection_alias: 35,
        spawn_marker_alias: 42,
        preparation_stage: -1,
        stage_after_preparation: -1,
        replace_collection: false,
        actor_count: 1,
        spawn_slot_count: 1,
        start_stage: 611,
        end_stage: -1,
        candidates: MTNL01_DAVID,
    },
];

const SFM04_DEATHCLAW: &[CandidateSpec] = &[CandidateSpec {
    source_actor: 0x075338,
    spawn_slot: 0,
}];
const SFM04_WAVES: &[WaveSpec] = &[WaveSpec {
    source_wave: 0x4F60B0,
    source_wave_eid: "WaveTypeDeathclaw",
    source_wave_contracts: &[SourceWaveContract {
        source_wave: 0x4F60B0,
        source_wave_eid: "WaveTypeDeathclaw",
        candidates: SFM04_DEATHCLAW,
    }],
    collection_alias: 50,
    spawn_marker_alias: 13,
    preparation_stage: 300,
    stage_after_preparation: -1,
    replace_collection: false,
    actor_count: 1,
    spawn_slot_count: 1,
    start_stage: 330,
    end_stage: -1,
    candidates: SFM04_DEATHCLAW,
}];

const QUEST_SPECS: &[QuestSpec] = &[
    QuestSpec {
        source_quest: 0x10AE02,
        source_quest_eid: "SFM04_Organic",
        waves: SFM04_WAVES,
    },
    QuestSpec {
        source_quest: 0x052A62,
        source_quest_eid: "EN05_CombatCourse",
        waves: EN05_COMBAT_WAVES,
    },
    QuestSpec {
        source_quest: 0x4E0719,
        source_quest_eid: "FS02_MQ_Reassembly",
        waves: FS02_WAVES,
    },
    QuestSpec {
        source_quest: 0x031163,
        source_quest_eid: "MTNS01_Intro",
        waves: MTNS01_WAVES,
    },
    QuestSpec {
        source_quest: 0x045A40,
        source_quest_eid: "MTNL01_Raiders",
        waves: MTNL01_WAVES,
    },
    QuestSpec {
        source_quest: 0x41A39D,
        source_quest_eid: "W05_MQ_003P_Muscle",
        waves: MQ003_WAVES,
    },
    QuestSpec {
        source_quest: 0x42F31B,
        source_quest_eid: "W05_MQR_203P",
        waves: MQR203_WAVES,
    },
    QuestSpec {
        source_quest: 0x40571C,
        source_quest_eid: "W05_MQS_203P",
        waves: MQS203_WAVES,
    },
    QuestSpec {
        source_quest: 0x5A0675,
        source_quest_eid: "COMP_Quest_Intro_Astronaut_CrashSpawnQuest",
        waves: ASTRONAUT_WAVES,
    },
    QuestSpec {
        source_quest: 0x574625,
        source_quest_eid: "COMP_Quest_Intro_Full_Beckett",
        waves: BECKETT_NARROW_ESCAPE_WAVES,
    },
    QuestSpec {
        source_quest: 0x7D6A80,
        source_quest_eid: "Burn_BountyHunt_GruntHunt",
        waves: BURN_GRUNT_WAVES,
    },
];

#[cfg(test)]
mod legacy_quest_tests {
    use super::*;

    #[test]
    fn legacy_quest_wave_contracts_match_source_waves() {
        let wave = &SFM04_WAVES[0];
        assert_eq!(
            (wave.source_wave, wave.candidates[0].source_actor),
            (0x4F60B0, 0x075338)
        );
        assert_eq!((wave.collection_alias, wave.spawn_marker_alias), (50, 13));
        assert_eq!((wave.actor_count, wave.spawn_slot_count), (1, 1));
        assert_eq!(
            (wave.preparation_stage, wave.start_stage, wave.end_stage),
            (300, 330, -1)
        );
        assert_eq!(wave.stage_after_preparation, -1);
        assert_eq!(wave.source_wave_contracts[0].candidates, wave.candidates);
        assert!(!wave.replace_collection);
        assert!(
            QUEST_SPECS
                .iter()
                .any(|quest| quest.source_quest == 0x10AE02 && quest.waves == SFM04_WAVES)
        );

        let rows = EN05_COMBAT_WAVES
            .iter()
            .map(|wave| {
                (
                    wave.collection_alias,
                    wave.spawn_marker_alias,
                    wave.preparation_stage,
                    wave.actor_count,
                    wave.replace_collection,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            rows,
            vec![
                (6, 8, 20, 3, false),
                (6, 8, 32, 3, true),
                (13, 8, 42, 1, false),
                (6, 8, 42, 2, true),
            ]
        );
        assert_eq!(EN05_COMBAT_WAVES[2].candidates[0].source_actor, 0x43C423);
        assert_eq!(EN05_COMBAT_WAVES[2].spawn_slot_count, 1);
        assert!(
            EN05_LACKEYS
                .iter()
                .all(|candidate| candidate.source_actor == 0x15E058)
        );
        assert!(
            EN05_COMBAT_WAVES
                .iter()
                .all(|wave| wave.stage_after_preparation == -1)
        );

        for (wave, collection, marker, start, stop) in [
            (&MTNS01_WAVES[0], 101, 86, 401, 420),
            (&FS02_WAVES[0], 22, 25, 205, 260),
        ] {
            assert_eq!(
                (wave.collection_alias, wave.spawn_marker_alias),
                (collection, marker)
            );
            assert_eq!(
                (wave.preparation_stage, wave.start_stage, wave.end_stage),
                (start, start, stop)
            );
            assert_eq!((wave.actor_count, wave.spawn_slot_count), (3, 3));
            assert_eq!(wave.source_wave_contracts[0].candidates, wave.candidates);
            assert_eq!(wave.stage_after_preparation, -1);
            for slot in 0..3 {
                assert!(
                    wave.candidates
                        .iter()
                        .any(|candidate| candidate.spawn_slot == slot)
                );
            }
        }
        assert_eq!(MTNS01_WAVES[0].source_wave, 0x43D199);
        assert_eq!(FS02_WAVES[0].source_wave, 0x4F60F1);

        let rows = MTNL01_WAVES
            .iter()
            .map(|wave| {
                (
                    wave.source_wave,
                    wave.candidates[0].source_actor,
                    wave.collection_alias,
                    wave.spawn_marker_alias,
                    wave.start_stage,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            rows,
            vec![
                (0x43C0C8, 0x48F2AC, 25, 40, 211),
                (0x43C0CC, 0x48F2AD, 26, 41, 511),
                (0x43C0CD, 0x4EA3FB, 35, 42, 611),
            ]
        );
        for wave in MTNL01_WAVES {
            assert_eq!(wave.actor_count, 1);
            assert_eq!(wave.spawn_slot_count, 1);
            assert_eq!(wave.source_wave_contracts[0].candidates, wave.candidates);
            assert_eq!(wave.end_stage, -1);
            assert!(!wave.replace_collection);
        }
        assert!(
            QUEST_SPECS
                .iter()
                .any(|q| q.source_quest == 0x045A40 && q.waves == MTNL01_WAVES)
        );
    }
}

impl Fixup for MaterializeFo76LocalEncounterWavesFixup {
    fn name(&self) -> &'static str {
        "materialize_fo76_local_encounter_waves"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        session.source_id().is_some()
            && session
                .source_schema()
                .is_ok_and(|schema| schema.record_def("WAVE").is_some())
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let source_schema = session
            .source_schema()
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let source_slot = session
            .source_slot_opt()
            .ok_or_else(|| FixupError::HandleError("source plugin missing".into()))?;
        let source_plugin = source_slot.parsed.plugin_name.clone();
        let source_plugin_sym = mapper.interner.intern(&source_plugin);
        let target_plugin = session.target_slot().parsed.plugin_name.clone();
        let target_masters = session.target_masters().to_vec();
        let qust_sig = SigCode::from_str("QUST")
            .map_err(|error| FixupError::SchemaError(error.to_string()))?;
        let target_quests = session
            .form_keys_of_sig(qust_sig, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?
            .into_iter()
            .collect::<FxHashSet<_>>();

        let mut attached = 0u32;
        let mut present = 0u32;
        let mut unresolved = 0u32;
        for quest in QUEST_SPECS {
            let source_quest = FormKey {
                local: quest.source_quest,
                plugin: source_plugin_sym,
            };
            if let Err(reason) = validate_source_contract(
                session,
                source_schema.as_ref(),
                source_plugin_sym,
                quest,
                mapper.interner,
            ) {
                unresolved += 1;
                warn(&mut report, mapper.interner, quest.source_quest, reason);
                continue;
            }

            let Some(target_quest) = mapper.lookup(source_quest) else {
                unresolved += 1;
                warn(
                    &mut report,
                    mapper.interner,
                    quest.source_quest,
                    "quest_unmapped",
                );
                continue;
            };
            if !target_quests.contains(&target_quest) {
                unresolved += 1;
                warn(
                    &mut report,
                    mapper.interner,
                    quest.source_quest,
                    "target_quest_missing",
                );
                continue;
            }

            let Some(target_waves) = map_target_waves(quest, source_plugin_sym, mapper) else {
                unresolved += 1;
                warn(
                    &mut report,
                    mapper.interner,
                    quest.source_quest,
                    "wave_actor_unmapped",
                );
                continue;
            };
            let Some(script_vmad) = materializer_script_vmad(
                target_quest,
                &target_waves,
                &target_masters,
                &target_plugin,
                mapper.interner,
            ) else {
                unresolved += 1;
                warn(
                    &mut report,
                    mapper.interner,
                    quest.source_quest,
                    "materializer_vmad_encode_failed",
                );
                continue;
            };

            let mut outcome = None;
            let visits = session
                .patch_all_subrecords_bytes(&target_quest, "VMAD", |bytes| {
                    if outcome.is_some() {
                        outcome = Some(AttachResult::Conflict("multiple_vmad_subrecords"));
                        return false;
                    }
                    let result = attach_script_bytes(bytes, MATERIALIZER_SCRIPT_NAME, &script_vmad);
                    outcome = Some(result);
                    result == AttachResult::Changed
                })
                .map_err(|error| FixupError::HandleError(error.to_string()))?;

            match outcome {
                Some(AttachResult::Changed) if visits == 1 => {
                    report.records_changed += 1;
                    attached += 1;
                }
                Some(AttachResult::AlreadyPresent) => present += 1,
                Some(AttachResult::Conflict(reason)) => {
                    unresolved += 1;
                    warn(&mut report, mapper.interner, quest.source_quest, reason);
                }
                _ => {
                    unresolved += 1;
                    warn(
                        &mut report,
                        mapper.interner,
                        quest.source_quest,
                        "quest_vmad_missing",
                    );
                }
            }
        }

        report.message = Some(mapper.interner.intern(&format!(
            "fo76_local_encounter_waves:attached={attached};present={present};unresolved={unresolved};quests={}",
            QUEST_SPECS.len()
        )));
        Ok(report)
    }
}

fn validate_source_contract(
    session: &mut PluginSession,
    source_schema: &crate::schema::AuthoringSchema,
    source_plugin: crate::sym::Sym,
    quest: &QuestSpec,
    interner: &StringInterner,
) -> Result<(), &'static str> {
    let quest_key = FormKey {
        local: quest.source_quest,
        plugin: source_plugin,
    };
    let quest_record = session
        .source_record_decoded(&quest_key, source_schema, interner)
        .map_err(|_| "source_quest_unreadable")?;
    if quest_record.sig.0 != *b"QUST"
        || quest_record
            .eid
            .and_then(|eid| interner.resolve(eid))
            .is_none_or(|eid| eid != quest.source_quest_eid)
    {
        return Err("source_quest_contract_mismatch");
    }

    let mut checked_waves = FxHashSet::default();
    let mut checked_actors = FxHashSet::default();
    for wave in quest.waves {
        if wave.source_wave_contracts.is_empty() {
            if checked_waves.insert(wave.source_wave) {
                validate_source_wave_identity(
                    session,
                    source_schema,
                    source_plugin,
                    wave.source_wave,
                    wave.source_wave_eid,
                    interner,
                )?;
            }
        } else {
            for source_wave in wave.source_wave_contracts {
                if checked_waves.insert(source_wave.source_wave) {
                    validate_exact_source_wave(
                        session,
                        source_schema,
                        source_plugin,
                        source_wave,
                        interner,
                    )?;
                }
            }
        }
        for candidate in wave.candidates {
            if !checked_actors.insert(candidate.source_actor) {
                continue;
            }
            let actor_key = FormKey {
                local: candidate.source_actor,
                plugin: source_plugin,
            };
            let actor_record = session
                .source_record_decoded(&actor_key, source_schema, interner)
                .map_err(|_| "source_wave_actor_unreadable")?;
            if actor_record.sig.0 != *b"NPC_" {
                return Err("source_wave_actor_not_npc");
            }
        }
    }
    Ok(())
}

fn validate_source_wave_identity(
    session: &mut PluginSession,
    source_schema: &crate::schema::AuthoringSchema,
    source_plugin: crate::sym::Sym,
    source_wave: u32,
    source_wave_eid: &str,
    interner: &StringInterner,
) -> Result<(), &'static str> {
    let wave_key = FormKey {
        local: source_wave,
        plugin: source_plugin,
    };
    let wave_record = session
        .source_record_decoded(&wave_key, source_schema, interner)
        .map_err(|_| "source_wave_unreadable")?;
    if wave_record.sig.0 != *b"WAVE"
        || wave_record
            .eid
            .and_then(|eid| interner.resolve(eid))
            .is_none_or(|eid| eid != source_wave_eid)
    {
        return Err("source_wave_contract_mismatch");
    }
    Ok(())
}

fn validate_exact_source_wave(
    session: &mut PluginSession,
    source_schema: &crate::schema::AuthoringSchema,
    source_plugin: crate::sym::Sym,
    contract: &SourceWaveContract,
    interner: &StringInterner,
) -> Result<(), &'static str> {
    validate_source_wave_identity(
        session,
        source_schema,
        source_plugin,
        contract.source_wave,
        contract.source_wave_eid,
        interner,
    )?;
    let wave_key = FormKey {
        local: contract.source_wave,
        plugin: source_plugin,
    };
    let wave_record = session
        .source_record_decoded(&wave_key, source_schema, interner)
        .map_err(|_| "source_wave_unreadable")?;
    let candidates = source_wave_candidates(&wave_record, source_plugin, interner)
        .ok_or("source_wave_candidates_unreadable")?;
    if candidates != contract.candidates {
        return Err("source_wave_candidates_mismatch");
    }
    Ok(())
}

fn source_wave_candidates(
    wave_record: &crate::record::Record,
    source_plugin: crate::sym::Sym,
    interner: &StringInterner,
) -> Option<Vec<CandidateSpec>> {
    let mut wavd_fields = wave_record
        .fields
        .iter()
        .filter(|entry| entry.sig.0 == *b"WAVD");
    let wavd = wavd_fields.next()?;
    if wavd_fields.next().is_some() {
        return None;
    }
    let crate::record::FieldValue::List(rows) = &wavd.value else {
        return None;
    };
    rows.iter()
        .map(|row| {
            let crate::record::FieldValue::Struct(fields) = row else {
                return None;
            };
            let mut source_actor = None;
            let mut spawn_slot = None;
            for (name, value) in fields {
                match interner.resolve(*name)? {
                    "wave_encounter_definitions_spawn_reference" => {
                        if source_actor.is_some() {
                            return None;
                        }
                        source_actor = source_wave_actor_local(value, source_plugin);
                        if source_actor.is_none() {
                            return None;
                        }
                    }
                    "wave_encounter_definitions_spawn_order" => {
                        if spawn_slot.is_some() {
                            return None;
                        }
                        spawn_slot = field_value_to_i32(value);
                    }
                    _ => {}
                }
            }
            Some(CandidateSpec {
                source_actor: source_actor?,
                spawn_slot: spawn_slot?,
            })
        })
        .collect()
}

fn source_wave_actor_local(
    value: &crate::record::FieldValue,
    source_plugin: crate::sym::Sym,
) -> Option<u32> {
    let crate::record::FieldValue::FormKey(form_key) = value else {
        return None;
    };
    (form_key.plugin == source_plugin).then_some(form_key.local)
}

fn field_value_to_i32(value: &crate::record::FieldValue) -> Option<i32> {
    match value {
        crate::record::FieldValue::Int(value) => i32::try_from(*value).ok(),
        crate::record::FieldValue::Uint(value) => i32::try_from(*value).ok(),
        _ => None,
    }
}

fn map_target_waves(
    quest: &QuestSpec,
    source_plugin: crate::sym::Sym,
    mapper: &FormKeyMapper,
) -> Option<Vec<TargetWaveSpec>> {
    quest
        .waves
        .iter()
        .map(|wave| {
            let candidates = wave
                .candidates
                .iter()
                .map(|candidate| {
                    mapper
                        .lookup(FormKey {
                            local: candidate.source_actor,
                            plugin: source_plugin,
                        })
                        .map(|target| (target, candidate.spawn_slot))
                })
                .collect::<Option<Vec<_>>>()?;
            Some(TargetWaveSpec {
                collection_alias: wave.collection_alias,
                spawn_marker_alias: wave.spawn_marker_alias,
                preparation_stage: wave.preparation_stage,
                stage_after_preparation: wave.stage_after_preparation,
                replace_collection: wave.replace_collection,
                actor_count: wave.actor_count,
                spawn_slot_count: wave.spawn_slot_count,
                candidates,
            })
        })
        .collect()
}

fn materializer_script_vmad(
    target_quest: FormKey,
    waves: &[TargetWaveSpec],
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<Vec<u8>> {
    if waves.is_empty() {
        return None;
    }
    let mut candidate_forms = Vec::new();
    let mut candidate_wave_indices = Vec::new();
    let mut candidate_spawn_slots = Vec::new();
    for (wave_index, wave) in waves.iter().enumerate() {
        for (form, spawn_slot) in &wave.candidates {
            candidate_forms.push(object_value(*form, -1, interner)?);
            candidate_wave_indices.push(i32::try_from(wave_index).ok()?);
            candidate_spawn_slots.push(*spawn_slot);
        }
    }

    let properties = vec![
        object_array_property(
            "WaveCollections",
            waves
                .iter()
                .map(|wave| object_value(target_quest, wave.collection_alias, interner))
                .collect::<Option<Vec<_>>>()?,
        ),
        object_array_property(
            "WaveSpawnMarkers",
            waves
                .iter()
                .map(|wave| object_value(target_quest, wave.spawn_marker_alias, interner))
                .collect::<Option<Vec<_>>>()?,
        ),
        int_array_property(
            "WavePreparationStages",
            waves.iter().map(|wave| wave.preparation_stage).collect(),
        ),
        int_array_property(
            "StageAfterPreparation",
            waves
                .iter()
                .map(|wave| wave.stage_after_preparation)
                .collect(),
        ),
        bool_array_property(
            "ReplaceCollectionOnPrepare",
            waves.iter().map(|wave| wave.replace_collection).collect(),
        ),
        int_array_property(
            "WaveActorCounts",
            waves.iter().map(|wave| wave.actor_count).collect(),
        ),
        int_array_property(
            "WaveSpawnSlotCounts",
            waves.iter().map(|wave| wave.spawn_slot_count).collect(),
        ),
        object_array_property("CandidateForms", candidate_forms),
        int_array_property("CandidateWaveIndices", candidate_wave_indices),
        int_array_property("CandidateSpawnSlots", candidate_spawn_slots),
    ];

    build_vmad_bytes_from_payload(
        &serde_json::json!({
            "Version": VMAD_VERSION,
            "Object Format": VMAD_OBJECT_FORMAT,
            "Scripts": [{
                "ScriptName": MATERIALIZER_SCRIPT_NAME,
                "Flags": 0,
                "Properties": properties,
            }],
        }),
        target_masters,
        target_plugin,
    )
}

fn object_array_property(name: &str, values: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({
        "propertyName": name,
        "Type": "Array of Object",
        "Flags": VMAD_PROPERTY_FLAG_EDITED,
        "Value": values,
    })
}

fn int_array_property(name: &str, values: Vec<i32>) -> serde_json::Value {
    serde_json::json!({
        "propertyName": name,
        "Type": "Array of Int32",
        "Flags": VMAD_PROPERTY_FLAG_EDITED,
        "Value": values,
    })
}

fn bool_array_property(name: &str, values: Vec<bool>) -> serde_json::Value {
    serde_json::json!({
        "propertyName": name,
        "Type": "Array of Bool",
        "Flags": VMAD_PROPERTY_FLAG_EDITED,
        "Value": values,
    })
}

fn object_value(
    form_key: FormKey,
    alias: i32,
    interner: &StringInterner,
) -> Option<serde_json::Value> {
    let plugin = interner.resolve(form_key.plugin)?;
    Some(serde_json::json!({
        "Alias": alias,
        "FormID": {
            "reference": {
                "plugin": plugin,
                "object_id": format!("{:06X}", form_key.local),
            },
        },
    }))
}

fn warn(report: &mut FixupReport, interner: &StringInterner, source_quest: u32, reason: &str) {
    report.warnings.push(interner.intern(&format!(
        "materialize_fo76_local_encounter_waves:{source_quest:06X}:{reason}"
    )));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_collections_are_replaced_only_at_prior_wave_endpoints() {
        assert_eq!(MQ003_WAVES[1].preparation_stage, MQ003_WAVES[0].end_stage);
        assert!(MQ003_WAVES[1].replace_collection);
        assert_eq!(MQ003_WAVES[1].stage_after_preparation, 725);
        assert_eq!(MQS203_WAVES[2].preparation_stage, MQS203_WAVES[1].end_stage);
        assert!(MQS203_WAVES[2].replace_collection);
        assert!(
            MQ003_WAVES[0..1]
                .iter()
                .chain(MQR203_WAVES.iter())
                .chain(MQS203_WAVES[0..2].iter())
                .all(|wave| !wave.replace_collection)
        );
        assert!(
            QUEST_SPECS
                .iter()
                .flat_map(|quest| quest.waves)
                .filter(|wave| wave.stage_after_preparation >= 0)
                .all(|wave| wave.source_wave == 0x59BBDD)
        );
    }

    #[test]
    fn exact_wave_validation_rejects_actor_refs_from_another_plugin() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("SeventySix.esm");
        let other_plugin = interner.intern("Fallout4.esm");
        let local_actor = crate::record::FieldValue::FormKey(FormKey {
            local: 0x075335,
            plugin: source_plugin,
        });
        let foreign_actor = crate::record::FieldValue::FormKey(FormKey {
            local: 0x075335,
            plugin: other_plugin,
        });

        assert_eq!(
            source_wave_actor_local(&local_actor, source_plugin),
            Some(0x075335)
        );
        assert_eq!(source_wave_actor_local(&foreign_actor, source_plugin), None);
    }

    #[test]
    fn generated_binding_is_idempotent_and_keeps_disabled_spawn_contract() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let target_quest = FormKey {
            local: 0x41A39D,
            plugin,
        };
        let waves = vec![TargetWaveSpec {
            collection_alias: 17,
            spawn_marker_alias: 43,
            preparation_stage: -1,
            stage_after_preparation: -1,
            replace_collection: false,
            actor_count: 4,
            spawn_slot_count: 4,
            candidates: vec![
                (
                    FormKey {
                        local: 0x08E624,
                        plugin,
                    },
                    0,
                ),
                (
                    FormKey {
                        local: 0x31B1FF,
                        plugin,
                    },
                    1,
                ),
            ],
        }];
        let script_vmad =
            materializer_script_vmad(target_quest, &waves, &[], "SeventySix.esm", &interner)
                .expect("materializer VMAD");
        let mut quest_vmad = build_vmad_bytes_from_payload(
            &serde_json::json!({
                "Version": VMAD_VERSION,
                "Object Format": VMAD_OBJECT_FORMAT,
                "Scripts": [],
            }),
            &[],
            "SeventySix.esm",
        )
        .expect("empty quest VMAD");

        assert_eq!(
            attach_script_bytes(&mut quest_vmad, MATERIALIZER_SCRIPT_NAME, &script_vmad),
            AttachResult::Changed
        );
        assert_eq!(
            attach_script_bytes(&mut quest_vmad, MATERIALIZER_SCRIPT_NAME, &script_vmad),
            AttachResult::AlreadyPresent
        );
        assert_eq!(u16::from_le_bytes([quest_vmad[4], quest_vmad[5]]), 1);
    }

    #[test]
    fn beckett_narrow_escape_production_spec_builds_attachable_vmad() {
        let interner = StringInterner::new();
        let source_plugin = interner.intern("SeventySix.esm");
        let target_plugin = interner.intern("Converted.esp");
        let mut mapper = FormKeyMapper::new(
            [],
            crate::formkey_mapper::MapperOptions {
                output_plugin_name: "Converted.esp".to_string(),
                ..Default::default()
            },
            &interner,
        );
        for source_actor in [0x53C52F, 0x53C531, 0x588087] {
            mapper.add_mapping(
                FormKey {
                    local: source_actor,
                    plugin: source_plugin,
                },
                FormKey {
                    local: source_actor,
                    plugin: target_plugin,
                },
            );
        }

        let waves = map_target_waves(
            QUEST_SPECS
                .iter()
                .find(|quest| quest.source_quest == 0x574625)
                .expect("Narrow Escape production quest spec"),
            source_plugin,
            &mapper,
        )
        .expect("mapped Narrow Escape wave");
        let target_quest = FormKey {
            local: 0x574625,
            plugin: target_plugin,
        };
        let script_vmad =
            materializer_script_vmad(target_quest, &waves, &[], "Converted.esp", &interner)
                .expect("Narrow Escape materializer VMAD");
        let mut quest_vmad = build_vmad_bytes_from_payload(
            &serde_json::json!({
                "Version": VMAD_VERSION,
                "Object Format": VMAD_OBJECT_FORMAT,
                "Scripts": [],
            }),
            &[],
            "Converted.esp",
        )
        .expect("empty quest VMAD");

        assert_eq!(
            attach_script_bytes(&mut quest_vmad, MATERIALIZER_SCRIPT_NAME, &script_vmad),
            AttachResult::Changed
        );
        assert_eq!(
            attach_script_bytes(&mut quest_vmad, MATERIALIZER_SCRIPT_NAME, &script_vmad),
            AttachResult::AlreadyPresent
        );
        assert_eq!(u16::from_le_bytes([quest_vmad[4], quest_vmad[5]]), 1);
    }
}
