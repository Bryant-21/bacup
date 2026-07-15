//! Fo76Fo4Hook — FO76→FO4 pair-level record hook.
//!
//! Ports the retired Python `Fo76ToFo4Hooks` implementation.
//!
//! # Behaviors ported
//!
//! 1. **Global field drop** (`pre_translate`) — removes subrecords whose four-byte
//!    sig matches any entry in `GLOBAL_DROP_SIGS`. Maps to `_GLOBAL_DROP_FIELDS`
//!    in the Python source.
//!
//! 2. **Synthetic-source-field marking** — `synthetic_source_fields()` returns the
//!    set of field names the translator should treat as synthesized (not decoded
//!    from the source ESP) for certain record types. Exposed as a pure method; the
//!    orchestrator calls it during setup. No `Record` mutation needed.
//!
//! 3. **Effects key routing** (`pre_translate` metadata, via `PairCtx`) — for
//!    ALCH/ENCH/SPEL/PERK records carrying an `Effects`-bearing subrecord, certain
//!    field names must be re-keyed. The routing table is expressed as a pure method
//!    `translate_effects_key`; the orchestrator applies it during field dispatch.
//!    No `Record` mutation needed here.
//!
//! `pre_process_source` is a no-op in the Python source and has no Rust equivalent.

use super::model_paths;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::translator::pair_hook::{HookResult, PairCtx, PairHook};
use esp_authoring_core::plugin_runtime::build_vmad_bytes_from_payload;
use smallvec::SmallVec;

/// Four-byte subrecord sigs to drop from every record before translation.
///
/// Mirrors `_GLOBAL_DROP_FIELDS` in `fo76_to_fo4.py`. The Python names are
/// YAML object-level field names that map 1-to-1 to subrecord sigs except
/// where noted:
///
/// | Python field             | Subrecord sig |
/// |--------------------------|---------------|
/// | ObjectPlacementDefaults  | OPDS (dropped as raw) |
/// | VersionControl           | VCTX          |
/// | FormVersion              | FVER          |
/// | Fallout76MajorRecordFlags| FL76          |
/// | MajorRecordFlagsRaw      | FLWR          |
/// | MaxItemID                | MIID          |
/// | MAGF                     | MAGF          |
/// | CODV                     | CODV          |
///
/// Note: Python field names that do not map directly to 4-char sigs are
/// represented here by their canonical subrecord equivalents. Orchestrator
/// must apply the same list when processing YAML-level keys by name.
const GLOBAL_DROP_SIGS: &[[u8; 4]] = &[
    *b"VCTX", // VersionControl
    *b"FVER", // FormVersion
    *b"FL76", // Fallout76MajorRecordFlags
    *b"FLWR", // MajorRecordFlagsRaw
    *b"MIID", // MaxItemID
    *b"MAGF", // MAGF (direct sig)
    *b"CODV", // CODV (direct sig)
    *b"OPDS", // ObjectPlacementDefaults
];

const TESLA_CANNON_BASE_MODEL: &str = "weapons/teslacannon/weapon_teslacannon.nif";
const CHINESE_STEALTH_ARMA_EDITOR_ID: &str = "AA_ArmorChineseStealth";

const FO76_UPPER_BODY_SKIN_BIPED_MASK: u64 =
    (1 << (41 - 30)) | (1 << (42 - 30)) | (1 << (43 - 30)) | (1 << (44 - 30)) | (1 << (45 - 30));
const FO76_ARMA_XFLG_HAS_UPPER_BODY_SKIN: u64 = 1 << 1;
const FO4_PIPBOY_BIPED_MASK: u64 = 1 << (60 - 30);

const EMPTY_SCOL_STAT_FIELD_SIGS: &[[u8; 4]] = &[
    *b"EDID", *b"VMAD", *b"OBND", *b"PTRN", *b"MODL", *b"MODT", *b"MODC", *b"MODS", *b"MODF",
    *b"FULL",
];

const DESTRUCTIBLE_GROUP_SIGS: &[[u8; 4]] = &[
    *b"DEST", *b"DAMC", *b"DSTD", *b"DSTA", *b"DMDL", *b"DMDT", *b"DMDC", *b"DMDS", *b"DSTF",
    *b"HGLB", *b"ENLT", *b"ENLS", *b"AUUV",
];

const WRLD_RUNTIME_TABLE_SIGS: &[[u8; 4]] = &[
    *b"RNAM", // large-reference table
    *b"MHDT", // max-height table
    *b"OFST", // offset table
    *b"CLSZ", // cell-size table
];

const QUST_ALIAS_SIGS: &[[u8; 4]] = &[
    *b"ALST", *b"ALID", *b"FNAM", *b"ALFI", *b"ALFR", *b"ALUA", *b"ALFA", *b"KNAM", *b"ALRT",
    *b"ALEQ", *b"ALEA", *b"ALCO", *b"ALCA", *b"ALCL", *b"ALNA", *b"ALNT", *b"ALFE", *b"ALFD",
    *b"ALCC", *b"CTDA", *b"CIS1", *b"CIS2", *b"KSIZ", *b"KWDA", *b"COCT", *b"CNTO", *b"COED",
    *b"SPOR", *b"OCOR", *b"GWOR", *b"ECOR", *b"ALLA", *b"ALDN", *b"ALFV", *b"ALDI", *b"ALSP",
    *b"ALFC", *b"ALPC", *b"VTCK", *b"ALED", *b"ALLS", *b"ALFL", *b"ALCS", *b"ALMI",
];
const QUST_ALIAS_ANCHOR_SIGS: &[[u8; 4]] = &[*b"ALST", *b"ALLS", *b"ALCS"];
const QUST_ALIAS_OPTIONAL_FLAG: u32 = 0x2;
const QUST_OBJECTIVE_TARGET_CONDITION_SIGS: &[[u8; 4]] = &[*b"CTDA", *b"CIS1", *b"CIS2"];
// QUST subrecords stripped from FO76 input before FO4 translation. Most are
// FO76-only chunks FO4 does not accept. ALFE/ALFD are FO4-known, but FO76
// event alias fills can fault FO4's ALFD resolver after the incompatible quest
// event scope is stripped, so the alias row is kept without its event-fill data.
const QUST_DROP_SIGS: &[[u8; 4]] = &[
    *b"ACBS", *b"ALFC", *b"ALFE", *b"ALFD", *b"ALSO", *b"ATTR", *b"COED", *b"DTGT", *b"ESAV",
    *b"ESCE", *b"ESCS", *b"ESDA", *b"ESRP", *b"ESRV", *b"KNAM", *b"NAM8", *b"QUCF", *b"SCCM",
    *b"SCFC", *b"SDCT", *b"SPPI", *b"SPPT", *b"TRAE", *b"VNAM",
];

/// Highest FO4 condition function id currently represented in the FO4 schema.
///
/// FO76 CTDA/CTDT subrecords can carry FO76-only function ids. The FO4 CK
/// indexes its condition-function table with those ids while loading and can
/// crash before it has a chance to report a warning.
const FO4_MAX_KNOWN_CONDITION_FUNCTION_ID: u16 = 817;
const FO76_IS_IN_INTERIOR_ACOUSTIC_SPACE_CONDITION_FUNCTION_ID: u16 = 276;
const FO4_IS_IN_INTERIOR_CONDITION_FUNCTION_ID: u16 = 300;
/// FO76 `GetIsCurrentLocationExact` (844, > FO4's 817 max). It takes an LCTN in
/// Parameter #1; FO4 `GetInCurrentLocation` (359) is the closest compatible gate.
const FO76_GET_IS_CURRENT_LOCATION_EXACT_CONDITION_FUNCTION_ID: u16 = 844;
const FO4_GET_IN_CURRENT_LOCATION_CONDITION_FUNCTION_ID: u16 = 359;
const RD01_ENC04_ASSASSIN_NPC_FORM_ID: u32 = 0x78BD9B;
const CS_RAIDER_01_MELEE_FORM_ID: u32 = 0x047165;
const CS_RAIDER_RANGED_FORM_ID: u32 = 0x03183B;
/// FO76-only `IsQuestActive` (876, > FO4's 817 max). It takes a QUST in
/// Parameter #1 and is compared `== 1`, so it maps value-identically onto FO4
/// `GetQuestRunning` (56). Remapped before the incompatibility drop so the
/// gating condition survives instead of being stripped (which would leave the
/// owning record — e.g. a loading screen — unconditionally eligible).
const FO76_IS_QUEST_ACTIVE_CONDITION_FUNCTION_ID: u16 = 876;
const FO4_GET_QUEST_RUNNING_CONDITION_FUNCTION_ID: u16 = 56;
/// Every FO76-only condition-function id that `normalize_fo76_raw_condition_functions`
/// rewrites to an FO4 equivalent. Consumed by the
/// `drop_untranslatable_loadscreen_records` fixup so it does NOT treat a
/// remapped function as untranslatable. Keep in sync with the remaps below.
pub(crate) const FO76_REMAPPED_CONDITION_FUNCTION_IDS: &[u16] = &[
    FO76_IS_IN_INTERIOR_ACOUSTIC_SPACE_CONDITION_FUNCTION_ID,
    FO76_GET_IS_CURRENT_LOCATION_EXACT_CONDITION_FUNCTION_ID,
    FO76_IS_QUEST_ACTIVE_CONDITION_FUNCTION_ID,
];
/// FO76-only condition ids below FO4's max id. The FO4 CK still treats these as
/// blank condition functions while loading, so the max-id guard is not enough.
/// 596 is an FO76-only function carried with a `$73808CE`-style Parameter #1 on
/// BS01 Brotherhood dialogue INFOs; xEdit renders it as `<Unknown:param>` and
/// the FO4 CK indexes its blank slot while loading (4 INFO records).
const FO76_ONLY_CONDITION_FUNCTION_IDS_UNDER_FO4_MAX: &[u16] =
    &[2, 3, 105, 371, 579, 596, 692, 730, 737];
// CK rejects exterior CELL parameters for this COBJ condition and can crash while loading.
const FO4_COBJ_EXTERIOR_CELL_REJECTED_CONDITION_FUNCTION_ID: u16 = 310;
// FO76 Function 67 carries source-side function-info/base-object values that
// FO4 CK tries to resolve from its own Function Info table while loading.
const FO76_FUNCTION_INFO_CONDITION_FUNCTION_ID: u16 = 67;
// FO4 condition functions whose Parameter #1 is a QUST FormID
// (wbDefinitionsFO4.pas, Paramtype1: ptQuest). A FO76 QUST referenced here may
// be dropped/unconverted, leaving Parameter #1 = NULL. xEdit then reports the
// CTDA as "Parameter #1 -> Found NULL, expected QUST". The condition can't be
// retargeted (there is no surviving quest), so the whole CTDA is dropped.
const FO4_QUEST_PARAMETER_1_CONDITION_FUNCTION_IDS: &[u16] = &[56, 58, 59, 543, 629, 664];
// CTDA "Run On" value (bytes [20..24]) for "Quest Alias": the condition resolves
// through an alias index against the owning quest.
const CTDA_RUN_ON_QUEST_ALIAS: u32 = 5;
// Record types whose CTDA carries an owning quest context (xEdit resolves it
// from the record container or QNAM/PNAM-style owner field).
const QUEST_CONTEXT_CONDITION_RECORD_SIGS: &[[u8; 4]] =
    &[*b"QUST", *b"SCEN", *b"PACK", *b"INFO", *b"DIAL"];
// GetIsAliasRef / alias-index parameter. It is valid only when xEdit can resolve
// an owning quest context and that quest has the referenced alias id.
const FO4_QUEST_ALIAS_PARAMETER_1_CONDITION_FUNCTION_IDS: &[u16] = &[566];
// FO76 nuke-zone check (849, > FO4's 817 max — always dropped as untranslatable).
// Gates leveled-list entries to nuke-irradiated zones (radiation suits, glowing
// variants) that never occur in FO4.
const FO76_NUKE_ZONE_CONDITION_FUNCTION_ID: u16 = 849;
// `GetGlobalValue` (74, shared FO4/FO76). FO76 gates seasonal/event leveled
// entries behind globals such as `Festive_Holiday_Enabled` that stay 0 in FO4.
const GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID: u16 = 74;
const FO76_MASTER_NAME: &str = "SeventySix.esm";
const FO4_MASTER_NAME: &str = "Fallout4.esm";
const FO76_WORKSHOP_WORKBENCH_ALL_TYPE: u32 = 0x89383D;

const FO4_WORKSHOP_WORKBENCH_EXTERIOR: u32 = 0x05A0C8;
const FO4_WORKSHOP_WORKBENCH_FURNITURE: u32 = 0x05B5E3;
const FO4_WORKSHOP_WORKBENCH_DECORATIONS: u32 = 0x08280B;
const FO4_WORKSHOP_WORKBENCH_POWER: u32 = 0x05A0CA;
const FO4_WORKSHOP_WORKBENCH_CRAFTING: u32 = 0x12E2C8;
const FO4_WORKSHOP_WORKBENCH_SETTLEMENT: u32 = 0x246F85;

const FO76_WORKSHOP_CATEGORY_APPLIANCES: u32 = 0x04422C;
const FO76_WORKSHOP_CATEGORY_BEDS: u32 = 0x04640E;
const FO76_WORKSHOP_CATEGORY_BLUEPRINTS: u32 = 0x046411;
const FO76_WORKSHOP_CATEGORY_CEILING_DECOR: u32 = 0x4249B6;
const FO76_WORKSHOP_CATEGORY_CHAIRS: u32 = 0x04642D;
const FO76_WORKSHOP_CATEGORY_CONTAINERS: u32 = 0x046433;
const FO76_WORKSHOP_CATEGORY_CRAFTING: u32 = 0x046471;
const FO76_WORKSHOP_CATEGORY_DEFENSE: u32 = 0x386054;
const FO76_WORKSHOP_CATEGORY_DISPLAYS: u32 = 0x060B21;
const FO76_WORKSHOP_CATEGORY_DOORS: u32 = 0x061FA1;
const FO76_WORKSHOP_CATEGORY_FLOOR_DECOR: u32 = 0x062011;
const FO76_WORKSHOP_CATEGORY_FLOORS: u32 = 0x06201B;
const FO76_WORKSHOP_CATEGORY_FOOD: u32 = 0x06201D;
const FO76_WORKSHOP_CATEGORY_GENERATORS: u32 = 0x06201E;
const FO76_WORKSHOP_CATEGORY_LIGHTS: u32 = 0x06201F;
const FO76_WORKSHOP_CATEGORY_MISC_STRUCTURES: u32 = 0x11C420;
const FO76_WORKSHOP_CATEGORY_POWER_CONNECTORS: u32 = 0x08032C;
const FO76_WORKSHOP_CATEGORY_RESOURCES: u32 = 0x095A38;
const FO76_WORKSHOP_CATEGORY_ROOFS: u32 = 0x12882C;
const FO76_WORKSHOP_CATEGORY_SHELTERS: u32 = 0x5A60A0;
const FO76_WORKSHOP_CATEGORY_SHELVES: u32 = 0x12882D;
const FO76_WORKSHOP_CATEGORY_STAIRS: u32 = 0x12882E;
const FO76_WORKSHOP_CATEGORY_TABLES: u32 = 0x1573C7;
const FO76_WORKSHOP_CATEGORY_TURRETS_TRAPS: u32 = 0x05294F;
const FO76_WORKSHOP_CATEGORY_VENDORS: u32 = 0x12882F;
const FO76_WORKSHOP_CATEGORY_WALL_DECOR: u32 = 0x1573EE;
const FO76_WORKSHOP_CATEGORY_WALLS: u32 = 0x1573F0;
const FO76_WORKSHOP_CATEGORY_WATER: u32 = 0x1573F7;
const FO76_WORKSHOP_CATEGORY_DWELLERS: u32 = 0x54EB71;
const FO76_WORKSHOP_CATEGORY_PETS: u32 = 0x411B84;
const FO76_WORKSHOP_CATEGORY_QUEST: u32 = 0x5895EE;
const FO76_WORKSHOP_CATEGORY_MAIN_STRUCTURE: u32 = 0x8229E6;
const FO76_WORKSHOP_CATEGORY_MAIN_FURNITURE: u32 = 0x8229E2;
const FO76_WORKSHOP_CATEGORY_MAIN_DECORATIONS: u32 = 0x8229E3;
const FO76_WORKSHOP_CATEGORY_MAIN_DEFENSE: u32 = 0x8229DF;
const FO76_WORKSHOP_CATEGORY_MAIN_POWER: u32 = 0x8229E0;
const FO76_WORKSHOP_CATEGORY_MAIN_RESOURCES: u32 = 0x8229E1;
const FO76_WORKSHOP_CATEGORY_MAIN_STORAGE: u32 = 0x8229E5;
const FO76_WORKSHOP_CATEGORY_MAIN_UTILITY: u32 = 0x822A19;
const FO76_WORKSHOP_CATEGORY_MAIN_DWELLERS: u32 = 0x8229E7;
const FO76_WORKSHOP_CATEGORY_MAIN_QUEST: u32 = 0x8229DD;
const FO76_CAPS_FORM_ID: u32 = 0x00000F;
const FO4_WORKBENCH_DATA_LEN: usize = 2;
const FO4_MGEF_DATA_LEN: usize = 152;
const FO76_MGEF_DATA_LEN: usize = 160;
const FO76_MGEF_DATA_WITHOUT_FLAGS2_LEN: usize = 156;
const FO76_MGEF_DATA_FLAGS2_OFFSET: usize = 4;
const FO76_MGEF_DATA_FLAGS2_END: usize = 8;
const FO4_MGEF_DATA_ARCHETYPE_OFFSET: usize = 64;
const FO4_MAX_MGEF_ARCHETYPE: u32 = 49;
const FO4_MGEF_ARCHETYPE_SCRIPT: u32 = 1;
const FO4_MGEF_ARCHETYPE_STAGGER: u32 = 33;
const FO76_MGEF_ARCHETYPE_TURBO_FERT: u32 = 50;
const FO76_MGEF_ARCHETYPE_CORPSE_HIGHLIGHT: u32 = 51;
const FO76_MGEF_ARCHETYPE_STUN: u32 = 52;
const FO76_DIAL_CATEGORY_DETECTION: u8 = 4;
const FO76_DIAL_CATEGORY_MISCELLANEOUS: u8 = 5;
const FO4_DIAL_CATEGORY_DETECTION: u8 = 5;
const FO4_DIAL_CATEGORY_MISCELLANEOUS: u8 = 7;
const SCEN_PLAYER_RESPONSE_SIGS: [[u8; 4]; 4] = [*b"PTOP", *b"NTOP", *b"NETO", *b"QTOP"];
const SCEN_NPC_RESPONSE_SIGS: [[u8; 4]; 4] = [*b"NPOT", *b"NNGT", *b"NNUT", *b"NQUT"];
const SCORCHED_STATUE_ACTI_EID: &str = "ScorchedStatue01";
const FO76_DAMAGE_TYPE_ROW_LEN: usize = 12;
const FO4_DAMAGE_TYPE_ROW_LEN: usize = 8;
const FO76_IDLM_UNKNOWN_5_FLAG: u8 = 0x20;
const FO4_MOVEMENT_SPEED_DATA_LEN: usize = 112;
const FURNITURE_INTERACTION_POINT_BITS: u32 = 0x003F_FFFF;
/// FURN/TERM `MNAM` "Has Model" flag (bit 30). When set, Interaction Point 0 is
/// backed by the model's default furniture marker.
const FURNITURE_HAS_MODEL_BIT: u32 = 0x4000_0000;
const FURNITURE_MARKER_PARAMETERS_ROW_LEN: usize = 24;
const FO4_POWER_ARMOR_FURNITURE_KEYWORD: u32 = 0x03430B;
const FO4_POWER_ARMOR_FIRST_PERSON_KEYWORD: u32 = 0x0A56D7;
const FO4_POWER_ARMOR_BATTERY_INSERT_ANIM_KEYWORD: u32 = 0x05BDA8;
const FO4_PLAYER_PATH_TO_FURNITURE_KEYWORD: u32 = 0x06D5BB;
const FO4_POWER_ARMOR_BATTERY_ITEM_KEYWORD: u32 = 0x05BDAA;
const POWER_ARMOR_BATTERY_INSERT_SCRIPT: &str = "PowerArmorBatteryInsertScript";
const FO4_VMAD_VERSION: u16 = 6;
const FO4_VMAD_OBJECT_FORMAT: u16 = 2;
const VMAD_PROPERTY_FLAG_EDITED: u8 = 1;
const FO4_LIGH_DATA_RADIUS_OFFSET: usize = 4;
const FO76_LIGH_DATA_VALUE_OFFSET: usize = 56;
const FO4_LIGH_DATA_NEAR_CLIP_OFFSET: usize = 24;
const FO4_LIGH_DATA_SCALAR_OFFSET: usize = 44;
const FO4_LIGH_DATA_EXPONENT_OFFSET: usize = 48;
const FO4_LIGH_DATA_VALUE_OFFSET: usize = 56;
const FO4_LIGH_DATA_WEIGHT_OFFSET: usize = 60;
const FO4_LIGH_DATA_LEN: usize = FO4_LIGH_DATA_WEIGHT_OFFSET + 4;
const FO4_LIGH_DATA_FLAGS_OFFSET: usize = 12;
const FO4_LIGH_DATA_FLICKER_INTENSITY_AMP_OFFSET: usize = 32;
const FO4_LIGH_DEFAULT_FADE: f32 = 1.0;
const FO4_LIGH_DEFAULT_SCALAR: f32 = 1.0;
const FO4_LIGH_DEFAULT_EXPONENT: f32 = 2.0;
const FO4_LIGH_DEFAULT_VALUE: u32 = 0;
const FO4_LIGH_DEFAULT_WEIGHT: f32 = 0.0;
const FO4_LIGH_MAX_SYNTHETIC_RADIUS: u32 = 2048;
const FO4_CAGE_BULB_GOBO_PATH: &str = "textures/effects/gobos/cagebulbgobo01_d.dds";
const FO4_CAGE_BULB_GOBO_MAX_RADIUS: u32 = 256;
const FO4_CAGE_BULB_GOBO_MIN_NEAR_CLIP: f32 = 32.0;
/// FO4 flicker-intensity-amplitude ceilings (`DATA` @ +32). FO76 authors this on
/// a far larger scale (flicker lights: median ~3, up to 30000) than FO4, whose
/// flicker lights never exceed 2.0 — and gobo/fire lights stay <= 0.8. Byte-copying
/// the FO76 value makes FO4 read a huge amplitude and strobe the light violently,
/// so it is clamped into FO4's own authored envelope (tighter for gobo lights).
const FO4_LIGH_MAX_FLICKER_INTENSITY_AMP: f32 = 2.0;
const FO4_LIGH_MAX_FLICKER_INTENSITY_AMP_GOBO: f32 = 0.8;
/// Near-clip floor for a converted light (`DATA` @ +24). FO76 leaves near clip at
/// 1.0 on almost every light; FO4's population median is ~7.2, and a near plane
/// this small wrecks shadow-map depth precision. FO76 carries no signal for FO4's
/// intended value, so a floor at the FO4 median is the safe heuristic.
const FO4_LIGH_MIN_NEAR_CLIP: f32 = 7.217;
/// `non_specular` bit (`DATA` flags @ +12). FO76 sets it on ~96% of lights (their
/// PBR pipeline drives specular separately); FO4 leaves it clear on ~82%. Byte-
/// copying it disables specular highlights on nearly every converted light, so it
/// is cleared to restore FO4-native specular.
const FO4_LIGH_FLAGS_NON_SPECULAR: u32 = 0x0000_8000;
/// FO4 defines LIGH `DATA` flag bits only up to 0x200000; FO76 sets higher bits
/// (0x400000+) that are meaningless in FO4. Mask the flags to FO4's defined range.
const FO4_LIGH_FLAGS_VALID_MASK: u32 = 0x003F_FFFF;

/// Record type sigs whose "Effects" subrecord group is treated as a synthetic
/// source field (i.e. the orchestrator synthesizes it rather than decoding it
/// directly from the source ESP).
///
/// RACE also synthesizes `BehaviorGraphDatas`, but that is a YAML-level
/// concept handled by the field-expansion transform, not a subrecord-drop.
pub const EFFECTS_SYNTHETIC_RECORD_SIGS: &[[u8; 4]] = &[*b"ALCH", *b"ENCH", *b"PERK", *b"SPEL"];

#[derive(Clone, Copy)]
enum ObjectModPropertyTarget {
    Weapon,
    Armor,
    Actor,
    Object,
}

const OBJECT_MOD_PROPERTY_ROW_LEN: usize = 24;
const OBJECT_MOD_PROPERTY_FUNCTION_TYPE_OFFSET: usize = 4;
const OBJECT_MOD_PROPERTY_ID_OFFSET: usize = 8;
const OBJECT_TEMPLATE_PROPERTY_COUNT_OFFSET: usize = 4;
const OBJECT_TEMPLATE_FIXED_HEADER_LEN: usize = 16;
const OBJECT_TEMPLATE_KEYWORD_COUNT_OFFSET: usize = 15;
const OBJECT_TEMPLATE_INCLUDE_ROW_LEN: usize = 7;
const OMOD_DATA_HEADER_LEN: usize = 20;
const OMOD_DATA_FORM_TYPE_OFFSET: usize = 10;
const OMOD_DATA_ATTACH_PARENT_SLOT_COUNT_LEN: usize = 4;
const OMOD_DATA_ITEM_ROW_LEN: usize = 4;
const OMOD_DATA_INCLUDE_ROW_LEN: usize = 7;
const FO76_MSTT_FORM_TYPE: u32 = 0x5454_534D;

/// FO76 OMOD `MNAM` (Target OMOD Keywords) entries with no useful FO4 role,
/// dropped entirely during translation. Values are SeventySix.esm object-ids
/// (master byte already stripped on decode). Append ARMO/ARMA appearance
/// mod-association keywords here when armor support lands.
const FO76_REDUNDANT_OMOD_TARGET_KEYWORD_OBJECT_IDS: &[u32] = &[
    0x0037_D0B2, // ma_Gun_Appearance (ModAssociation)
];
const PACK_PROCEDURE_TREE_BOUNDARY_SIGS: &[[u8; 4]] =
    &[*b"UNAM", *b"BNAM", *b"POBA", *b"POEA", *b"POCA"];
const ACTIVATION_CONDITION_SIGS: &[[u8; 4]] =
    &[*b"CNDC", *b"CITC", *b"CTDA", *b"CTDT", *b"CIS1", *b"CIS2"];

// PACK package-data location (PLDT/PLVD) `Type` values whose `Location Value`
// union is a quest-alias INDEX (Ref Alias / Loc Alias / Ref Collection Alias).
// The index is validated against the package's owning QUST (its QNAM). QUST alias
// ID anchors are preserved, but this record-local hook cannot prove a package's
// owning quest context survived with the matching alias, so package data aliases
// are still normalized to non-alias selectors below.
const PACK_LOCATION_ALIAS_TYPES: &[i32] = &[8, 9, 14];
// Benign non-alias replacement: "Near Package Start Location" — a self-relative
// location whose 4-byte value xEdit treats as cpIgnore (no external resolution).
const PACK_LOCATION_NEAR_PACKAGE_START_TYPE: i32 = 2;
// PTDA target `Type` whose `Target` union is a quest-alias INDEX.
const PACK_TARGET_ALIAS_TYPE: i32 = 4;
// Benign non-alias replacement: "Self" — needs no external reference.
const PACK_TARGET_SELF_TYPE: i32 = 6;

fn trim_nul_suffix(mut bytes: &[u8]) -> &[u8] {
    while matches!(bytes.last(), Some(0)) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

/// Hook result pair for effects key routing (field name sym → target key sym).
///
/// When `None`, no rerouting is needed. When `Some((field_sig, target_sig))`,
/// the orchestrator should use `target_sig` as the target subrecord sig for
/// the field identified by `field_sig`.
///
/// Mirrors `Fo76ToFo4Hooks::translate_effects_keys`.
pub struct EffectsKeyRoute {
    /// The field sig to match on the source record.
    pub field_sig: SubrecordSig,
    /// The target subrecord sig to emit.
    pub target_sig: SubrecordSig,
}

/// True when a CTDA function id has no FO4 equivalent: it exceeds FO4's max
/// known condition-function id (817), or it is a FO76-only id that falls under
/// that max but is still a blank slot in FO4. Keeping such a condition makes the
/// FO4 CK index a non-existent function-table entry and crash while loading.
pub(crate) fn is_fo4_incompatible_condition_function_id(function_id: u16) -> bool {
    function_id > FO4_MAX_KNOWN_CONDITION_FUNCTION_ID
        || FO76_ONLY_CONDITION_FUNCTION_IDS_UNDER_FO4_MAX.contains(&function_id)
}

pub(crate) const FO76_RADIO_FREQUENCY_NAMESPACE_OFFSET: f32 = 2048.0;

pub(crate) fn namespace_fo76_radio_frequency(frequency: &mut f32) -> bool {
    if !frequency.is_finite()
        || *frequency <= 0.0
        || *frequency >= FO76_RADIO_FREQUENCY_NAMESPACE_OFFSET
    {
        return false;
    }
    *frequency += FO76_RADIO_FREQUENCY_NAMESPACE_OFFSET;
    true
}

/// FO76→FO4 pair hook.
pub struct Fo76Fo4Hook;

impl Fo76Fo4Hook {
    fn arma_has_upper_body_skin(record: &Record) -> bool {
        record.sig.0 == *b"ARMA"
            && record.fields.iter().any(|entry| {
                entry.sig.0 == *b"XFLG"
                    && match entry.value {
                        FieldValue::Uint(flags) => flags & FO76_ARMA_XFLG_HAS_UPPER_BODY_SKIN != 0,
                        FieldValue::Int(flags) if flags >= 0 => {
                            (flags as u64) & FO76_ARMA_XFLG_HAS_UPPER_BODY_SKIN != 0
                        }
                        _ => false,
                    }
            })
    }

    fn normalize_arma_upper_body_skin_slots(record: &mut Record) {
        if !Self::arma_has_upper_body_skin(record) {
            return;
        }
        let Some(value) = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"BOD2")
            .map(|entry| &mut entry.value)
        else {
            return;
        };
        Self::remove_biped_slots(value, FO76_UPPER_BODY_SKIN_BIPED_MASK);
    }

    fn normalize_chinese_stealth_arma_pipboy_slot(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"ARMA"
            || !record.fields.iter().any(|entry| {
                if entry.sig.0 != *b"EDID" {
                    return false;
                }
                match &entry.value {
                    FieldValue::String(value) => interner
                        .resolve(*value)
                        .is_some_and(|value| value == CHINESE_STEALTH_ARMA_EDITOR_ID),
                    FieldValue::Bytes(value) => {
                        std::str::from_utf8(value).ok().is_some_and(|value| {
                            value.trim_end_matches('\0') == CHINESE_STEALTH_ARMA_EDITOR_ID
                        })
                    }
                    _ => false,
                }
            })
        {
            return;
        }
        if let Some(value) = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"BOD2")
            .map(|entry| &mut entry.value)
        {
            Self::remove_biped_slots(value, FO4_PIPBOY_BIPED_MASK);
        }
    }

    fn remove_biped_slots(value: &mut FieldValue, slots: u64) {
        match value {
            FieldValue::Uint(mask) => *mask &= !slots,
            FieldValue::Int(mask) if *mask >= 0 => *mask &= !(slots as i64),
            _ => {}
        }
    }

    fn namespace_radio_receiver_frequency(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"ACTI" {
            return;
        }
        for entry in &mut record.fields {
            if entry.sig.0 != *b"RADR" {
                continue;
            }
            match &mut entry.value {
                FieldValue::Bytes(bytes) if bytes.len() >= 8 => {
                    let mut frequency =
                        f32::from_le_bytes(bytes[4..8].try_into().expect("RADR frequency"));
                    if namespace_fo76_radio_frequency(&mut frequency) {
                        bytes[4..8].copy_from_slice(&frequency.to_le_bytes());
                    }
                }
                FieldValue::Struct(fields) => {
                    let Some((_, FieldValue::Float(frequency))) = fields
                        .iter_mut()
                        .find(|(name, _)| Self::struct_field_name_is(interner, *name, "Frequency"))
                    else {
                        continue;
                    };
                    namespace_fo76_radio_frequency(frequency);
                }
                _ => {}
            }
        }
    }

    fn is_convertible_workshop_cobj(
        interner: &crate::sym::StringInterner,
        record: &Record,
    ) -> bool {
        if record.sig.0 != *b"COBJ" {
            return false;
        }
        let Some(eid) = record.eid.and_then(|eid| interner.resolve(eid)) else {
            return false;
        };
        let eid = eid.to_ascii_lowercase();
        let has_workshop_token = eid.starts_with("workshop_") || eid.contains("_workshop_");
        let excluded_recipe_family = ["co_mod", "co_clothes", "co_cloths"]
            .iter()
            .any(|family| eid.starts_with(family) || eid.contains(&format!("_{family}")));
        has_workshop_token && !eid.starts_with("zzz") && !excluded_recipe_family
    }

    fn workshop_category_workbench(category: u32) -> Option<u32> {
        match category {
            FO76_WORKSHOP_CATEGORY_APPLIANCES
            | FO76_WORKSHOP_CATEGORY_BEDS
            | FO76_WORKSHOP_CATEGORY_CHAIRS
            | FO76_WORKSHOP_CATEGORY_CONTAINERS
            | FO76_WORKSHOP_CATEGORY_DISPLAYS
            | FO76_WORKSHOP_CATEGORY_SHELVES
            | FO76_WORKSHOP_CATEGORY_TABLES
            | FO76_WORKSHOP_CATEGORY_MAIN_FURNITURE
            | FO76_WORKSHOP_CATEGORY_MAIN_STORAGE => Some(FO4_WORKSHOP_WORKBENCH_FURNITURE),
            FO76_WORKSHOP_CATEGORY_CEILING_DECOR
            | FO76_WORKSHOP_CATEGORY_FLOOR_DECOR
            | FO76_WORKSHOP_CATEGORY_WALL_DECOR
            | FO76_WORKSHOP_CATEGORY_MAIN_DECORATIONS => Some(FO4_WORKSHOP_WORKBENCH_DECORATIONS),
            FO76_WORKSHOP_CATEGORY_GENERATORS
            | FO76_WORKSHOP_CATEGORY_LIGHTS
            | FO76_WORKSHOP_CATEGORY_POWER_CONNECTORS
            | FO76_WORKSHOP_CATEGORY_MAIN_POWER => Some(FO4_WORKSHOP_WORKBENCH_POWER),
            FO76_WORKSHOP_CATEGORY_CRAFTING | FO76_WORKSHOP_CATEGORY_MAIN_UTILITY => {
                Some(FO4_WORKSHOP_WORKBENCH_CRAFTING)
            }
            FO76_WORKSHOP_CATEGORY_RESOURCES
            | FO76_WORKSHOP_CATEGORY_VENDORS
            | FO76_WORKSHOP_CATEGORY_DWELLERS
            | FO76_WORKSHOP_CATEGORY_PETS
            | FO76_WORKSHOP_CATEGORY_MAIN_DWELLERS => Some(FO4_WORKSHOP_WORKBENCH_SETTLEMENT),
            FO76_WORKSHOP_CATEGORY_BLUEPRINTS
            | FO76_WORKSHOP_CATEGORY_DEFENSE
            | FO76_WORKSHOP_CATEGORY_DOORS
            | FO76_WORKSHOP_CATEGORY_FLOORS
            | FO76_WORKSHOP_CATEGORY_FOOD
            | FO76_WORKSHOP_CATEGORY_MISC_STRUCTURES
            | FO76_WORKSHOP_CATEGORY_ROOFS
            | FO76_WORKSHOP_CATEGORY_SHELTERS
            | FO76_WORKSHOP_CATEGORY_STAIRS
            | FO76_WORKSHOP_CATEGORY_TURRETS_TRAPS
            | FO76_WORKSHOP_CATEGORY_WALLS
            | FO76_WORKSHOP_CATEGORY_WATER
            | FO76_WORKSHOP_CATEGORY_QUEST
            | FO76_WORKSHOP_CATEGORY_MAIN_STRUCTURE
            | FO76_WORKSHOP_CATEGORY_MAIN_DEFENSE
            | FO76_WORKSHOP_CATEGORY_MAIN_RESOURCES
            | FO76_WORKSHOP_CATEGORY_MAIN_QUEST => Some(FO4_WORKSHOP_WORKBENCH_EXTERIOR),
            _ => None,
        }
    }

    fn infer_workshop_category(eid: &str) -> (u32, u32) {
        let eid = eid.to_ascii_lowercase();
        if eid.contains("campfire")
            || eid.contains("camp_fire")
            || eid.contains("firebarrel")
            || eid.contains("fire_barrel")
        {
            return (
                FO76_WORKSHOP_CATEGORY_LIGHTS,
                FO4_WORKSHOP_WORKBENCH_FURNITURE,
            );
        }
        let category = if eid.contains("walldecor") || eid.contains("wall_decor") {
            FO76_WORKSHOP_CATEGORY_WALL_DECOR
        } else if eid.contains("floordecor") || eid.contains("floor_decor") {
            FO76_WORKSHOP_CATEGORY_FLOOR_DECOR
        } else if eid.contains("ceilingdecor") || eid.contains("ceiling_decor") {
            FO76_WORKSHOP_CATEGORY_CEILING_DECOR
        } else if eid.contains("generator") {
            FO76_WORKSHOP_CATEGORY_GENERATORS
        } else if eid.contains("powerconnector") || eid.contains("power_connector") {
            FO76_WORKSHOP_CATEGORY_POWER_CONNECTORS
        } else if eid.contains("light") || eid.contains("lamp") {
            FO76_WORKSHOP_CATEGORY_LIGHTS
        } else if eid.contains("crafting") || eid.contains("workbench") {
            FO76_WORKSHOP_CATEGORY_CRAFTING
        } else if eid.contains("vendor") || eid.contains("vending") {
            FO76_WORKSHOP_CATEGORY_VENDORS
        } else if eid.contains("display") {
            FO76_WORKSHOP_CATEGORY_DISPLAYS
        } else if eid.contains("container") || eid.contains("stash") {
            FO76_WORKSHOP_CATEGORY_CONTAINERS
        } else if eid.contains("shelf") {
            FO76_WORKSHOP_CATEGORY_SHELVES
        } else if eid.contains("table") {
            FO76_WORKSHOP_CATEGORY_TABLES
        } else if eid.contains("chair") || eid.contains("seating") {
            FO76_WORKSHOP_CATEGORY_CHAIRS
        } else if eid.contains("bed") {
            FO76_WORKSHOP_CATEGORY_BEDS
        } else if eid.contains("appliance") || eid.contains("utility") {
            FO76_WORKSHOP_CATEGORY_APPLIANCES
        } else if eid.contains("turret") || eid.contains("trap") || eid.contains("defense") {
            FO76_WORKSHOP_CATEGORY_TURRETS_TRAPS
        } else if eid.contains("water") {
            FO76_WORKSHOP_CATEGORY_WATER
        } else if eid.contains("food") || eid.contains("crop") || eid.contains("plant") {
            FO76_WORKSHOP_CATEGORY_FOOD
        } else if eid.contains("resource") || eid.contains("collector") || eid.contains("producer")
        {
            FO76_WORKSHOP_CATEGORY_RESOURCES
        } else if eid.contains("door") {
            FO76_WORKSHOP_CATEGORY_DOORS
        } else if eid.contains("roof") {
            FO76_WORKSHOP_CATEGORY_ROOFS
        } else if eid.contains("stair") {
            FO76_WORKSHOP_CATEGORY_STAIRS
        } else if eid.contains("floor") || eid.contains("foundation") {
            FO76_WORKSHOP_CATEGORY_FLOORS
        } else if eid.contains("wall") {
            FO76_WORKSHOP_CATEGORY_WALLS
        } else if eid.contains("shelter") {
            FO76_WORKSHOP_CATEGORY_SHELTERS
        } else if eid.contains("pet") {
            FO76_WORKSHOP_CATEGORY_PETS
        } else if eid.contains("dweller") || eid.contains("ally") {
            FO76_WORKSHOP_CATEGORY_DWELLERS
        } else if eid.contains("quest") {
            FO76_WORKSHOP_CATEGORY_QUEST
        } else if eid.contains("decor") || eid.contains("rug") || eid.contains("statue") {
            FO76_WORKSHOP_CATEGORY_FLOOR_DECOR
        } else {
            FO76_WORKSHOP_CATEGORY_MISC_STRUCTURES
        };
        let workbench =
            Self::workshop_category_workbench(category).unwrap_or(FO4_WORKSHOP_WORKBENCH_EXTERIOR);
        (category, workbench)
    }

    fn normalize_workshop_cobj(interner: &crate::sym::StringInterner, record: &mut Record) {
        if !Self::is_convertible_workshop_cobj(interner, record) {
            return;
        }
        let has_created_object = record.fields.iter().any(|entry| {
            entry.sig.0 == *b"CNAM"
                && matches!(&entry.value, FieldValue::FormKey(form_key) if form_key.local != 0)
        });
        if !has_created_object {
            return;
        }
        let Some(bench_index) = record
            .fields
            .iter()
            .position(|entry| entry.sig.0 == *b"BNAM")
        else {
            return;
        };
        let FieldValue::FormKey(source_bench) = record.fields[bench_index].value else {
            return;
        };
        let source_plugin = interner.resolve(source_bench.plugin).unwrap_or_default();
        if !source_plugin.eq_ignore_ascii_case(FO76_MASTER_NAME) {
            return;
        }

        let eid = record
            .eid
            .and_then(|eid| interner.resolve(eid))
            .unwrap_or_default();
        let (category, workbench) = if source_bench.local == FO76_WORKSHOP_WORKBENCH_ALL_TYPE {
            Self::infer_workshop_category(eid)
        } else {
            let Some(workbench) = Self::workshop_category_workbench(source_bench.local) else {
                return;
            };
            (source_bench.local, workbench)
        };

        let category_form_key = FormKey {
            local: category,
            plugin: interner.intern(FO76_MASTER_NAME),
        };
        record.fields[bench_index].value = FieldValue::FormKey(FormKey {
            local: workbench,
            plugin: interner.intern(FO4_MASTER_NAME),
        });
        if record.fields.iter().all(|entry| entry.sig.0 != *b"FNAM") {
            record.fields.insert(
                bench_index + 1,
                FieldEntry {
                    sig: SubrecordSig(*b"FNAM"),
                    value: FieldValue::List(vec![FieldValue::FormKey(category_form_key)]),
                },
            );
        }
    }

    /// Drop all subrecords whose sig is in `GLOBAL_DROP_SIGS`.
    fn drop_global_fields(record: &mut Record) {
        record
            .fields
            .retain(|entry| !GLOBAL_DROP_SIGS.iter().any(|sig| entry.sig.0 == *sig));
    }

    fn convert_nif_backed_empty_scol_to_stat(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"SCOL"
            || record
                .fields
                .iter()
                .any(|entry| entry.sig.0 == *b"ONAM" && scol_onam_is_usable(&entry.value))
            || !record.fields.iter().any(|entry| {
                entry.sig.0 == *b"MODL" && field_value_has_non_empty_text(&entry.value, interner)
            })
        {
            return;
        }

        record.sig = SigCode(*b"STAT");
        record.fields.retain(|entry| {
            EMPTY_SCOL_STAT_FIELD_SIGS
                .iter()
                .any(|sig| entry.sig.0 == *sig)
        });
    }

    fn strip_wrld_runtime_tables(record: &mut Record) {
        if record.sig.0 != *b"WRLD" {
            return;
        }
        record.fields.retain(|entry| {
            !WRLD_RUNTIME_TABLE_SIGS
                .iter()
                .any(|sig| entry.sig.0 == *sig)
        });
    }

    fn normalize_npc_perk_entries(interner: &crate::sym::StringInterner, record: &mut Record) {
        if record.sig.0 != *b"NPC_" {
            return;
        }

        let source_plugin = record.form_key.plugin;
        let perk_key = interner.intern("Perk");
        let rank_key = interner.intern("Rank");
        for entry in &mut record.fields {
            if entry.sig.0 != *b"PRKR" {
                continue;
            }
            if let Some(value) =
                npc_perk_entry_value(&entry.value, source_plugin, perk_key, rank_key)
            {
                entry.value = value;
            }
        }
    }

    fn convert_or_drop_cell_combined_reference_index(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"CELL" {
            return;
        }

        let mut retained = smallvec::SmallVec::new();
        for mut entry in record.fields.drain(..) {
            if entry.sig.0 == *b"XCRI" {
                if let Some(converted) = convert_cell_xcri_to_fo4(&entry.value, interner) {
                    entry.value = converted;
                    retained.push(entry);
                }
                continue;
            }
            retained.push(entry);
        }
        record.fields = retained;
    }

    fn strip_qust_runtime_scopes(record: &mut Record) {
        if record.sig.0 != *b"QUST" {
            return;
        }

        let mut retained: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
        let mut after_next_alias_id = false;
        let mut after_objective_target = false;
        let mut in_objective = false;
        let mut current_alias_fnam_index: Option<usize> = None;
        let mut current_alias_lost_event_fill = false;
        for mut entry in record.fields.drain(..) {
            // ENAM is the FO76-only quest event scope; it has no FO4 equivalent.
            // VMAD is now retained (and FormKey-remapped by the schema-driven
            // mapper) so quest Papyrus script bindings survive; without it,
            // GetVMQuestVariable conditions cannot resolve their variable names.
            if entry.sig.0 == *b"ENAM" {
                after_objective_target = false;
                in_objective = false;
                continue;
            }
            if entry.sig.0 == *b"QOBJ" {
                in_objective = true;
            }
            if entry.sig.0 == *b"ANAM" {
                after_next_alias_id = true;
                after_objective_target = false;
                in_objective = false;
                retained.push(entry);
                continue;
            }
            // FO76 uses objective-scope SNAM for StageToSet; FO4 interprets an
            // unscoped SNAM as a SWF path, so it cannot cross this boundary.
            if in_objective && entry.sig.0 == *b"SNAM" {
                after_objective_target = false;
                continue;
            }
            // The alias chain (everything after the NextAliasID anchor) is
            // retained so the FO4 alias table is rebuilt; scenes, dialogue, and
            // packages resolve their alias references against it. Runtime-unsafe
            // or FO76-only alias subrecords are dropped by QUST_DROP_SIGS.
            if after_next_alias_id && QUST_ALIAS_SIGS.iter().any(|sig| entry.sig.0 == *sig) {
                after_objective_target = false;
                if QUST_ALIAS_ANCHOR_SIGS.iter().any(|sig| entry.sig.0 == *sig) {
                    current_alias_fnam_index = None;
                    current_alias_lost_event_fill = false;
                }
                if QUST_DROP_SIGS.iter().any(|sig| entry.sig.0 == *sig) {
                    if matches!(&entry.sig.0, b"ALFE" | b"ALFD") {
                        current_alias_lost_event_fill = true;
                        if let Some(index) = current_alias_fnam_index {
                            if let Some(fnam) = retained.get_mut(index) {
                                mark_qust_alias_fnam_optional(&mut fnam.value);
                            }
                        }
                    }
                    continue;
                }
                if entry.sig.0 == *b"FNAM" {
                    if current_alias_lost_event_fill {
                        mark_qust_alias_fnam_optional(&mut entry.value);
                    }
                    current_alias_fnam_index = Some(retained.len());
                }
                retained.push(entry);
                continue;
            }
            if entry.sig.0 == *b"QSTA" {
                after_objective_target = true;
                continue;
            }
            if after_objective_target
                && QUST_OBJECTIVE_TARGET_CONDITION_SIGS
                    .iter()
                    .any(|sig| entry.sig.0 == *sig)
            {
                continue;
            }
            after_objective_target = false;
            if QUST_DROP_SIGS.iter().any(|sig| entry.sig.0 == *sig) {
                continue;
            }
            retained.push(entry);
        }
        record.fields = retained;
    }

    fn drop_fo4_incompatible_conditions(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        Self::normalize_fo76_raw_condition_functions(record);
        let record_sig = record.sig.0;
        // When a CTDA is dropped, its trailing CIS1/CIS2 parameter strings must
        // be dropped with it: they immediately follow their owning condition and
        // FO4 rejects a CIS1/CIS2 that is not preceded by a CTDA (CK/xEdit report
        // it as an out-of-order subrecord, e.g. orphaned `BTXT CIS2` rows in TERM
        // body/menu condition groups).
        let mut dropping_condition_strings = false;
        record.fields.retain(|entry| match &entry.sig.0 {
            b"CTDA" | b"CTDT" => {
                let drop = Self::condition_function_id(interner, &entry.value).is_some_and(
                    |function_id| {
                        if Self::is_fo4_incompatible_condition_function_id(function_id) {
                            return true;
                        }
                        let parameter_1 =
                            Self::condition_parameter_1(interner, &entry.value).unwrap_or(0);
                        if function_id == FO76_FUNCTION_INFO_CONDITION_FUNCTION_ID
                            && parameter_1 != 0
                        {
                            return true;
                        }
                        if parameter_1 == 0
                            && FO4_QUEST_PARAMETER_1_CONDITION_FUNCTION_IDS.contains(&function_id)
                        {
                            return true;
                        }
                        if FO4_QUEST_ALIAS_PARAMETER_1_CONDITION_FUNCTION_IDS.contains(&function_id)
                            && !QUEST_CONTEXT_CONDITION_RECORD_SIGS.contains(&record_sig)
                        {
                            return true;
                        }
                        let run_on = Self::condition_run_on(interner, &entry.value).unwrap_or(0);
                        if run_on == CTDA_RUN_ON_QUEST_ALIAS
                            && !QUEST_CONTEXT_CONDITION_RECORD_SIGS.contains(&record_sig)
                        {
                            return true;
                        }
                        record_sig == *b"COBJ"
                            && function_id == FO4_COBJ_EXTERIOR_CELL_REJECTED_CONDITION_FUNCTION_ID
                            && parameter_1 != 0
                    },
                );
                dropping_condition_strings = drop;
                !drop
            }
            b"CIS1" | b"CIS2" => !dropping_condition_strings,
            _ => {
                dropping_condition_strings = false;
                true
            }
        });
        if record.fields.iter().any(|entry| entry.sig.0 == *b"CITC") {
            // CTDA rows may already be stale after generic condition translation.
            record.sync_condition_count();
        }
    }

    fn normalize_fo76_raw_condition_functions(record: &mut Record) {
        for entry in &mut record.fields {
            if !matches!(&entry.sig.0, b"CTDA" | b"CTDT") {
                continue;
            }
            let FieldValue::Bytes(bytes) = &mut entry.value else {
                continue;
            };
            let Some(function_id) = Self::raw_condition_function_id(bytes) else {
                continue;
            };
            if function_id == FO76_IS_IN_INTERIOR_ACOUSTIC_SPACE_CONDITION_FUNCTION_ID {
                Self::set_raw_condition_function_id(
                    bytes,
                    FO4_IS_IN_INTERIOR_CONDITION_FUNCTION_ID,
                );
            } else if function_id == FO76_GET_IS_CURRENT_LOCATION_EXACT_CONDITION_FUNCTION_ID {
                Self::set_raw_condition_function_id(
                    bytes,
                    FO4_GET_IN_CURRENT_LOCATION_CONDITION_FUNCTION_ID,
                );
            } else if function_id == FO76_IS_QUEST_ACTIVE_CONDITION_FUNCTION_ID {
                Self::set_raw_condition_function_id(
                    bytes,
                    FO4_GET_QUEST_RUNNING_CONDITION_FUNCTION_ID,
                );
            }
        }
    }

    fn is_fo4_incompatible_condition_function_id(function_id: u16) -> bool {
        is_fo4_incompatible_condition_function_id(function_id)
    }

    fn strip_pack_runtime_refs(record: &mut Record) {
        if record.sig.0 != *b"PACK" {
            return;
        }

        let mut stripped_condition_data = false;
        record.fields.retain(|entry| match &entry.sig.0 {
            b"CTDA" | b"CTDT" | b"CIS1" | b"CIS2" => {
                stripped_condition_data = true;
                false
            }
            _ => true,
        });

        if stripped_condition_data {
            for entry in &mut record.fields {
                if entry.sig.0 == *b"CITC" {
                    set_u32_zero(&mut entry.value);
                }
            }
        }
    }

    /// Neutralize PACK package-data location/target entries that reference a
    /// quest-alias INDEX when this stateless hook cannot validate the owning
    /// QUST's alias table. Rewrite the union-selecting `Type` to a benign
    /// non-alias kind (location -> Near Package Start, target -> Self) and zero
    /// the now-unused value. The struct is fixed-size, so the subrecord length,
    /// data-input count, and template ABI are unchanged.
    fn neutralize_dangling_package_alias_targets(record: &mut Record) {
        if record.sig.0 != *b"PACK" {
            return;
        }
        for entry in &mut record.fields {
            let (alias_types, replacement) = match &entry.sig.0 {
                b"PLDT" | b"PLVD" => (
                    PACK_LOCATION_ALIAS_TYPES,
                    PACK_LOCATION_NEAR_PACKAGE_START_TYPE,
                ),
                b"PTDA" => (
                    std::slice::from_ref(&PACK_TARGET_ALIAS_TYPE),
                    PACK_TARGET_SELF_TYPE,
                ),
                _ => continue,
            };
            let FieldValue::Bytes(bytes) = &mut entry.value else {
                continue;
            };
            if bytes.len() < 8 {
                continue;
            }
            let type_value = i32::from_le_bytes(bytes[0..4].try_into().unwrap());
            if !alias_types.contains(&type_value) {
                continue;
            }
            bytes[0..4].copy_from_slice(&replacement.to_le_bytes());
            // Zero the union value (bytes [4..8]); for the benign replacement
            // types this field is cpIgnore, so the prior alias index is dead.
            bytes[4..8].copy_from_slice(&0u32.to_le_bytes());
        }
    }

    fn map_fo76_fallback_package_procedure(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"PACK" {
            return;
        }

        let mut after_package_data_marker = false;
        for entry in &mut record.fields {
            if entry.sig.0 == *b"XNAM" {
                after_package_data_marker = true;
                continue;
            }
            if !after_package_data_marker || !matches!(&entry.sig.0, b"ANAM" | b"PNAM") {
                continue;
            }
            match &mut entry.value {
                FieldValue::Bytes(bytes) => {
                    let value = bytes
                        .as_slice()
                        .strip_suffix(&[0])
                        .unwrap_or(bytes.as_slice());
                    if value == b"Fallback" {
                        bytes.clear();
                        bytes.extend_from_slice(b"Sequence\0");
                    }
                }
                FieldValue::String(sym) if interner.resolve(*sym) == Some("Fallback") => {
                    *sym = interner.intern("Sequence");
                }
                _ => {}
            }
        }
    }

    fn normalize_fo76_pack_procedure_tree(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"PACK" {
            return;
        }

        let Some(tree_start) = record
            .fields
            .iter()
            .position(|entry| entry.sig.0 == *b"XNAM")
            .map(|index| index + 1)
        else {
            return;
        };
        let tree_end = record.fields[tree_start..]
            .iter()
            .position(|entry| PACK_PROCEDURE_TREE_BOUNDARY_SIGS.contains(&entry.sig.0))
            .map(|offset| tree_start + offset)
            .unwrap_or(record.fields.len());
        if tree_start >= tree_end {
            return;
        }

        let mut rebuilt = Vec::with_capacity(record.fields.len());
        rebuilt.extend(record.fields[..tree_start].iter().cloned());
        rebuilt.extend(Self::rewrite_pack_procedure_tree_entries(
            interner,
            &record.fields[tree_start..tree_end],
        ));
        rebuilt.extend(record.fields[tree_end..].iter().cloned());
        record.fields = smallvec::SmallVec::from_vec(rebuilt);
    }

    fn rewrite_pack_procedure_tree_entries(
        interner: &crate::sym::StringInterner,
        entries: &[FieldEntry],
    ) -> Vec<FieldEntry> {
        let mut out = Vec::with_capacity(entries.len());
        let mut carry_root: Vec<FieldEntry> = Vec::new();
        let mut group_start = 0usize;
        while group_start < entries.len() {
            if entries[group_start].sig.0 != *b"ANAM" {
                out.push(entries[group_start].clone());
                group_start += 1;
                continue;
            }

            let group_end = entries[group_start + 1..]
                .iter()
                .position(|entry| entry.sig.0 == *b"ANAM")
                .map(|offset| group_start + 1 + offset)
                .unwrap_or(entries.len());
            let group = &entries[group_start..group_end];
            group_start = group_end;

            let Some(branch_type) = Self::pack_tree_entry_text(interner, &group[0]) else {
                out.extend(group.iter().cloned());
                continue;
            };
            let is_procedure = branch_type == "Procedure";
            let mut branch_entry = group[0].clone();
            Self::map_pack_tree_branch_type_value(interner, &mut branch_entry);

            let mut prefix = Vec::new();
            let mut root = Vec::new();
            let mut procedure = Vec::new();
            let mut trailing = Vec::new();
            let mut procedure_started = false;
            let mut ignored_numeric_procedure = false;

            for entry in &group[1..] {
                if entry.sig.0 == *b"PRCB" {
                    root.push(entry.clone());
                    continue;
                }

                if entry.sig.0 == *b"PNAM" {
                    if let Some(mapped) = Self::fo76_pack_procedure_name(interner, entry) {
                        let mut mapped_entry = entry.clone();
                        Self::set_pack_tree_text_value(interner, &mut mapped_entry, mapped);
                        procedure.push(mapped_entry);
                        procedure_started = true;
                    } else {
                        ignored_numeric_procedure = true;
                    }
                    continue;
                }

                if procedure_started
                    && matches!(&entry.sig.0, b"FNAM" | b"PKC2" | b"PFO2" | b"PFOR")
                {
                    procedure.push(entry.clone());
                } else if ignored_numeric_procedure && entry.sig.0 == *b"PKC2" {
                    trailing.push(entry.clone());
                } else {
                    prefix.push(entry.clone());
                }
            }

            if is_procedure {
                if procedure.is_empty() {
                    out.extend(trailing);
                    if !root.is_empty() {
                        carry_root.extend(root);
                    }
                    continue;
                }
                out.push(branch_entry);
                out.extend(prefix);
                out.extend(procedure);
                out.extend(trailing);
                if !root.is_empty() {
                    carry_root.extend(root);
                }
            } else {
                out.push(branch_entry);
                out.extend(prefix);
                if !carry_root.is_empty() {
                    out.append(&mut carry_root);
                } else {
                    out.extend(root);
                }
                if !procedure.is_empty() {
                    out.push(Self::pack_tree_text_entry(interner, "ANAM", "Procedure"));
                    out.push(Self::pack_tree_u32_entry("CITC", 0));
                    out.extend(procedure);
                }
                out.extend(trailing);
            }
        }

        out.extend(carry_root);
        out
    }

    fn map_pack_tree_branch_type_value(
        interner: &crate::sym::StringInterner,
        entry: &mut FieldEntry,
    ) {
        let Some(value) = Self::pack_tree_entry_text(interner, entry) else {
            return;
        };
        if value == "Fallback" {
            Self::set_pack_tree_text_value(interner, entry, "Sequence");
        }
    }

    fn pack_tree_entry_text(
        interner: &crate::sym::StringInterner,
        entry: &FieldEntry,
    ) -> Option<String> {
        match &entry.value {
            FieldValue::Bytes(bytes) => {
                let value = trim_nul_suffix(bytes.as_slice());
                std::str::from_utf8(value).ok().map(str::to_owned)
            }
            FieldValue::String(sym) => interner.resolve(*sym).map(str::to_owned),
            _ => None,
        }
    }

    fn set_pack_tree_text_value(
        interner: &crate::sym::StringInterner,
        entry: &mut FieldEntry,
        value: &str,
    ) {
        entry.value = FieldValue::String(interner.intern(value));
    }

    fn pack_tree_text_entry(
        interner: &crate::sym::StringInterner,
        sig: &str,
        value: &str,
    ) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).expect("valid PACK procedure tree sig"),
            value: FieldValue::String(interner.intern(value)),
        }
    }

    fn pack_tree_u32_entry(sig: &str, value: u32) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).expect("valid PACK procedure tree sig"),
            value: FieldValue::Uint(value as u64),
        }
    }

    fn fo76_pack_procedure_name(
        interner: &crate::sym::StringInterner,
        entry: &FieldEntry,
    ) -> Option<&'static str> {
        let value = match &entry.value {
            FieldValue::Bytes(bytes) => trim_nul_suffix(bytes.as_slice()).to_vec(),
            FieldValue::String(sym) => interner.resolve(*sym)?.as_bytes().to_vec(),
            _ => return None,
        };
        match value.as_slice() {
            b"Trav" | b"Travel" => Some("Travel"),
            b"Sand" | b"Sandbox" => Some("Sandbox"),
            b"Foll" | b"Follow" => Some("Follow"),
            b"Wait" => Some("Wait"),
            b"Patr" | b"Patrol" => Some("Patrol"),
            b"Sit" => Some("Sit"),
            b"UseW" | b"UseWeapon" => Some("UseWeapon"),
            b"Rang" | b"Range" => Some("Range"),
            b"Unlo" | b"UnlockDoors" => Some("UnlockDoors"),
            b"Acti" | b"Activate" => Some("Activate"),
            b"Find" => Some("Find"),
            b"Esco" | b"Escort" => Some("Escort"),
            b"Hold" | b"HoldPosition" => Some("HoldPosition"),
            b"Slee" | b"Sleep" => Some("Sleep"),
            b"Guar" | b"Guard" => Some("Guard"),
            b"Eat" => Some("Eat"),
            b"Say" | b"ForceGreet" => Some("ForceGreet"),
            b"Flee" => Some("Flee"),
            b"Head" | b"Headtrack" => Some("Headtrack"),
            b"Orbi" | b"Orbit" => Some("Orbit"),
            b"UseI" | b"UseIdleMarker" => Some("UseIdleMarker"),
            // Procedures present in FO76 with an identical FO4 procedure name
            // (verified against vanilla Fallout4.esm). Without these mappings the
            // procedure-tree rewrite drops the item, producing CK "missing
            // procedure / missing procedure tree item" on the owning package.
            b"GuardArea" => Some("GuardArea"),
            b"Hover" => Some("Hover"),
            b"KeepAnEyeOn" => Some("KeepAnEyeOn"),
            b"LockDoors" => Some("LockDoors"),
            b"UseMagic" => Some("UseMagic"),
            b"Acquire" => Some("Acquire"),
            b"FollowTo" => Some("FollowTo"),
            _ => None,
        }
    }

    fn convert_or_drop_region_objects(interner: &crate::sym::StringInterner, record: &mut Record) {
        if record.sig.0 != *b"REGN" {
            return;
        }

        let mut retained = smallvec::SmallVec::new();
        for mut entry in record.fields.drain(..) {
            if entry.sig.0 == *b"RDOT" {
                if let Some(converted) =
                    crate::fo76_rdot::convert_fo76_regn_rdot_to_fo4(&entry.value, interner)
                {
                    entry.value = converted;
                    retained.push(entry);
                }
                continue;
            }
            retained.push(entry);
        }
        record.fields = retained;
    }

    fn convert_mgef_data_to_fo4_layout(record: &mut Record) {
        if record.sig.0 != *b"MGEF" {
            return;
        }
        for entry in &mut record.fields {
            if entry.sig.0 != *b"DATA" {
                continue;
            }
            let FieldValue::Bytes(bytes) = &mut entry.value else {
                continue;
            };
            match bytes.len() {
                FO76_MGEF_DATA_LEN => {
                    bytes.drain(FO76_MGEF_DATA_FLAGS2_OFFSET..FO76_MGEF_DATA_FLAGS2_END);
                    bytes.truncate(FO4_MGEF_DATA_LEN);
                }
                FO76_MGEF_DATA_WITHOUT_FLAGS2_LEN => {
                    bytes.truncate(FO4_MGEF_DATA_LEN);
                }
                _ => {}
            }
            Self::normalize_mgef_archetype(bytes.as_mut_slice());
        }
    }

    fn normalize_mgef_archetype(bytes: &mut [u8]) {
        let Some(chunk) =
            bytes.get_mut(FO4_MGEF_DATA_ARCHETYPE_OFFSET..FO4_MGEF_DATA_ARCHETYPE_OFFSET + 4)
        else {
            return;
        };
        let archetype = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        let normalized = match archetype {
            FO76_MGEF_ARCHETYPE_STUN => FO4_MGEF_ARCHETYPE_STAGGER,
            FO76_MGEF_ARCHETYPE_TURBO_FERT | FO76_MGEF_ARCHETYPE_CORPSE_HIGHLIGHT => {
                FO4_MGEF_ARCHETYPE_SCRIPT
            }
            value if value > FO4_MAX_MGEF_ARCHETYPE => FO4_MGEF_ARCHETYPE_SCRIPT,
            _ => return,
        };
        chunk.copy_from_slice(&normalized.to_le_bytes());
    }

    fn normalize_scen_player_dialogue_choices(record: &mut Record) {
        if record.sig.0 != *b"SCEN" {
            return;
        }

        let source_plugin = record.form_key.plugin;
        let old_fields: Vec<FieldEntry> = record.fields.drain(..).collect();
        let mut retained: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
        let mut action_row: Vec<FieldEntry> = Vec::new();
        let mut in_action_row = false;

        for entry in old_fields {
            if entry.sig.0 == *b"ANAM" {
                if in_action_row {
                    push_scen_action_with_fo4_choices(source_plugin, action_row, &mut retained);
                    action_row = Vec::new();
                }
                in_action_row = true;
            }

            if in_action_row {
                action_row.push(entry);
            } else {
                retained.push(entry);
            }
        }

        if in_action_row {
            push_scen_action_with_fo4_choices(source_plugin, action_row, &mut retained);
        }

        record.fields = retained;
    }

    fn normalize_dial_data_category(interner: &crate::sym::StringInterner, record: &mut Record) {
        if record.sig.0 != *b"DIAL" {
            return;
        }
        for entry in &mut record.fields {
            if entry.sig.0 != *b"DATA" {
                continue;
            }
            Self::normalize_dial_data_category_value(interner, &mut entry.value);
        }
    }

    fn normalize_dial_data_category_value(
        interner: &crate::sym::StringInterner,
        value: &mut FieldValue,
    ) {
        match value {
            FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
                bytes[1] = fo76_dial_category_to_fo4(bytes[1]);
            }
            FieldValue::Struct(fields) => {
                for (name, field_value) in fields {
                    if Self::struct_field_name_is(interner, *name, "category") {
                        normalize_u8_field_value(field_value, fo76_dial_category_to_fo4);
                        return;
                    }
                }
            }
            _ => {}
        }
    }

    fn strip_scorched_statue_activation_conditions(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"ACTI"
            || !record.eid.is_some_and(|eid| {
                interner
                    .resolve(eid)
                    .is_some_and(|eid| eid.eq_ignore_ascii_case(SCORCHED_STATUE_ACTI_EID))
            })
        {
            return;
        }

        record.fields.retain(|entry| {
            !ACTIVATION_CONDITION_SIGS
                .iter()
                .any(|sig| entry.sig.0 == *sig)
        });
    }

    /// FO76 stores QUST quest-data in a `DATA` subrecord; FO4 stores it in
    /// `DNAM` ("General"). The translation map drops FO76 `DATA`, so without this
    /// relayout every converted QUST loses its quest-data — including the
    /// `start_game_enabled` flag and quest type — and no quest auto-starts. That
    /// leaves alias-gated dialogue unreachable (NPCs show no Talk prompt) even
    /// though the DIAL/INFO records convert fine. Renaming DATA→DNAM here (before
    /// the map drops `DATA`) lets the translator carry the synthesized DNAM
    /// through unchanged.
    fn convert_qust_data_to_fo4_dnam(interner: &crate::sym::StringInterner, record: &mut Record) {
        if record.sig.0 != *b"QUST" {
            return;
        }
        // A QUST that already carries DNAM (a handful of FO76 records do) needs
        // no relayout.
        if record.fields.iter().any(|entry| entry.sig.0 == *b"DNAM") {
            return;
        }
        let dnam_sig = match SubrecordSig::from_str("DNAM") {
            Ok(s) => s,
            Err(_) => return,
        };
        // For now, only NPC-conversation ("dialogue"), holotape-container, and
        // radio-station quests are allowed to auto-run. EVERY other quest type is
        // disabled (SGE cleared): carrying over gameplay/event/main quests
        // auto-fires objectives (the FO76 location-discovery DING),
        // begin-on-quest-start scenes that seize actors and grey out Talk, and
        // player force-greets that leave the player locked "in conversation" and
        // unable to talk to anyone.
        let is_test_or_dev = qust_eid_is_test_or_dev(interner, record);
        let is_dialogue = !is_test_or_dev && qust_eid_is_dialogue_conversation(interner, record);
        let is_holotape = !is_test_or_dev && qust_eid_is_holotape(interner, record);
        let is_radio = !is_test_or_dev && qust_eid_is_radio(interner, record);
        for entry in record.fields.iter_mut() {
            if entry.sig.0 != *b"DATA" {
                continue;
            }
            let FieldValue::Bytes(bytes) = &entry.value else {
                continue;
            };
            let Some(mut dnam) = build_fo4_qust_dnam_from_fo76_data(bytes) else {
                return;
            };
            if fo76_qust_type_disables_start_game(dnam[8]) {
                suppress_quest_autostart(&mut dnam);
            } else if is_dialogue || is_holotape {
                // Dialogue quests were Story-Manager-started (now gone); holotape
                // containers must run so a Voice holotape's scene can play. Both
                // are benign (no begin-on-quest-start world scenes) and carry
                // has_dialogue_data, so force on.
                force_dialogue_quest_autostart(&mut dnam);
            } else if !is_radio {
                suppress_quest_autostart(&mut dnam);
            }
            // Radio: preserve FO76's own start flag — real stations ship
            // start-game-enabled; main-quest radio segments don't and stay off.
            entry.sig = dnam_sig;
            entry.value = FieldValue::Bytes(dnam);
            return;
        }
    }

    fn drop_perk_vmad(record: &mut Record) {
        if record.sig.0 != *b"PERK" {
            return;
        }
        record.fields.retain(|entry| entry.sig.0 != *b"VMAD");
    }

    fn strip_invalid_object_mod_properties(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        match &record.sig.0 {
            b"WEAP" => {
                strip_record_obts_properties(interner, record, ObjectModPropertyTarget::Weapon)
            }
            b"ARMO" => {
                strip_record_obts_properties(interner, record, ObjectModPropertyTarget::Armor)
            }
            b"NPC_" => {
                strip_record_obts_properties(interner, record, ObjectModPropertyTarget::Actor)
            }
            b"OMOD" => strip_omod_data_properties(interner, record),
            _ => {}
        }
    }

    /// Drop FO76-only mod-association keywords from OMOD `MNAM` (Target OMOD
    /// Keywords) that have no FO4 equivalent. Runs in `pre_translate` so the
    /// FO76 keyword never reaches the mapper. If a filter empties an `MNAM`
    /// array the now-empty subrecord is removed entirely.
    fn strip_redundant_omod_target_keywords(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"OMOD" || omod_has_material_swap_data(interner, record) {
            return;
        }
        let source_plugin = interner.intern(FO76_MASTER_NAME);
        for entry in &mut record.fields {
            if entry.sig.0 != *b"MNAM" {
                continue;
            }
            filter_formkey_array_value(
                &mut entry.value,
                source_plugin,
                FO76_REDUNDANT_OMOD_TARGET_KEYWORD_OBJECT_IDS,
            );
        }
        record
            .fields
            .retain(|entry| entry.sig.0 != *b"MNAM" || !formkey_array_value_is_empty(&entry.value));
    }

    fn strip_material_omod_models(interner: &crate::sym::StringInterner, record: &mut Record) {
        if record.sig.0 != *b"OMOD" || !omod_has_material_swap_data(interner, record) {
            return;
        }

        record
            .fields
            .retain(|entry| !matches!(&entry.sig.0, b"MODL" | b"MODB" | b"MODT" | b"MODF"));
    }

    fn strip_tesla_cannon_receiver_model(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"OMOD"
            || !record.fields.iter().any(|entry| {
                entry.sig.0 == *b"INDX"
                    && match &entry.value {
                        FieldValue::Uint(index) => *index == 0,
                        FieldValue::Int(index) => *index == 0,
                        FieldValue::Bytes(bytes) => bytes.as_slice() == [0],
                        _ => false,
                    }
            })
        {
            return;
        }

        let uses_base_model = record.fields.iter().any(|entry| {
            if entry.sig.0 != *b"MODL" {
                return false;
            }
            let path = match &entry.value {
                FieldValue::String(path) => interner.resolve(*path),
                FieldValue::Bytes(bytes) => std::str::from_utf8(bytes).ok(),
                _ => None,
            };
            path.is_some_and(|path| {
                path.trim_end_matches('\0')
                    .replace('\\', "/")
                    .eq_ignore_ascii_case(TESLA_CANNON_BASE_MODEL)
            })
        });
        if !uses_base_model {
            return;
        }

        // FO4 has no OMOD INDX semantics and otherwise attaches a second copy
        // of the full weapon body instead of reusing the base model.
        record.fields.retain(|entry| {
            !matches!(
                &entry.sig.0,
                b"MODL"
                    | b"MODB"
                    | b"MODT"
                    | b"MODS"
                    | b"MODF"
                    | b"MODD"
                    | b"XFLG"
                    | b"ENLT"
                    | b"ENLS"
                    | b"AUUV"
                    | b"INDX"
            )
        });
    }

    fn normalize_omod_material_swap_functions(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"OMOD" {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 != *b"DATA" {
                continue;
            }
            let target = omod_property_target(&entry.value, interner);
            let Some(property_id) = material_swap_property_id(target) else {
                continue;
            };
            set_omod_data_property_function_type(interner, &mut entry.value, property_id, 2);
        }
    }

    fn normalize_npc_raw_form_refs(interner: &crate::sym::StringInterner, record: &mut Record) {
        if record.sig.0 != *b"NPC_" {
            return;
        }

        for entry in &mut record.fields {
            match &entry.sig.0 {
                b"SNAM" => {
                    if let Some(value) =
                        npc_faction_value(interner, &entry.value, record.form_key.plugin)
                    {
                        entry.value = value;
                    }
                }
                b"CNTO" => {
                    if let Some(value) =
                        npc_container_value(interner, &entry.value, record.form_key.plugin)
                    {
                        entry.value = value;
                    }
                }
                b"INAM" => {
                    if let Some(value) = source_form_key_value(&entry.value, record.form_key.plugin)
                    {
                        entry.value = value;
                    }
                }
                _ => {}
            }
        }
    }

    fn normalize_rd01_assassin_combat_style(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"NPC_" || record.form_key.local != RD01_ENC04_ASSASSIN_NPC_FORM_ID {
            return;
        }
        if record.eid.and_then(|sym| interner.resolve(sym)) != Some("RD01_Enc04_Assassin") {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 != *b"ZNAM" {
                continue;
            }
            if let FieldValue::FormKey(fk) = &mut entry.value {
                if fk.local == CS_RAIDER_01_MELEE_FORM_ID {
                    fk.local = CS_RAIDER_RANGED_FORM_ID;
                }
            }
        }
    }

    fn normalize_cont_raw_form_refs(interner: &crate::sym::StringInterner, record: &mut Record) {
        if record.sig.0 != *b"CONT" && record.sig.0 != *b"FURN" {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 == *b"CNTO"
                && let Some(value) =
                    npc_container_value(interner, &entry.value, record.form_key.plugin)
            {
                entry.value = value;
            }
        }
    }

    fn strip_info_editor_id(record: &mut Record) {
        if record.sig.0 == *b"INFO" {
            record.fields.retain(|entry| entry.sig.0 != *b"EDID");
        }
    }

    fn strip_orphan_term_conditions(record: &mut Record) {
        if record.sig.0 != *b"TERM" {
            return;
        }

        let old_fields: Vec<FieldEntry> = record.fields.drain(..).collect();
        let mut retained = smallvec::SmallVec::new();
        let mut condition_anchor_active = false;
        let mut condition_group_started = false;
        let mut keep_condition_strings = false;

        for entry in old_fields {
            match &entry.sig.0 {
                b"BSIZ" | b"ISIZ" => {
                    condition_anchor_active = false;
                    condition_group_started = false;
                    keep_condition_strings = false;
                    retained.push(entry);
                }
                b"BTXT" | b"ITXT" => {
                    condition_anchor_active = true;
                    condition_group_started = false;
                    keep_condition_strings = false;
                    retained.push(entry);
                }
                b"CTDA" | b"CTDT" => {
                    keep_condition_strings = condition_anchor_active;
                    if keep_condition_strings {
                        condition_group_started = true;
                        retained.push(entry);
                    }
                }
                b"CIS1" | b"CIS2" => {
                    if keep_condition_strings {
                        retained.push(entry);
                    }
                }
                _ => {
                    if condition_group_started {
                        condition_anchor_active = false;
                        condition_group_started = false;
                    }
                    keep_condition_strings = false;
                    retained.push(entry);
                }
            }
        }

        record.fields = retained;
    }

    fn normalize_refr_map_marker_tnam(record: &mut Record) {
        if record.sig.0 != *b"REFR" {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 == *b"TNAM"
                && let Some(source_type) = field_value_to_u16(&entry.value)
            {
                let target_type = fo76_map_marker_type_to_fo4(source_type);
                entry.value = bytes_value(&[target_type, 0]);
            }
        }
    }

    fn rename_furniture_marker_parameters(record: &mut Record) {
        if record.sig.0 != *b"FURN" {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 == *b"ZNAM" {
                entry.sig = SubrecordSig::from_str("SNAM").expect("SNAM is a valid signature");
            }
        }
    }

    fn strip_zero_health_cont_destructibles(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"CONT" {
            return;
        }

        let old_fields: Vec<FieldEntry> = record.fields.drain(..).collect();
        let mut retained = smallvec::SmallVec::new();
        let mut dropping_zero_health_group = false;

        for entry in old_fields {
            if entry.sig.0 == *b"DEST" {
                dropping_zero_health_group = destructible_header_health(interner, &entry.value)
                    .is_some_and(|health| health == 0);
                if dropping_zero_health_group {
                    continue;
                }
            } else if dropping_zero_health_group {
                if DESTRUCTIBLE_GROUP_SIGS.contains(&entry.sig.0) {
                    continue;
                }
                dropping_zero_health_group = false;
            }

            retained.push(entry);
        }

        record.fields = retained;
    }

    fn normalize_note_scene_ref(record: &mut Record) {
        if record.sig.0 != *b"NOTE" {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 == *b"SNAM"
                && let Some(value) = source_form_key_value(&entry.value, record.form_key.plugin)
            {
                entry.value = value;
            }
        }
    }

    fn strip_fo76_only_subrecord_tails(record: &mut Record) {
        match &record.sig.0 {
            b"FURN" | b"TERM" => {
                truncate_raw_subrecord(record, b"WBDT", FO4_WORKBENCH_DATA_LEN);
            }
            b"MOVT" => {
                truncate_raw_subrecord(record, b"SPED", FO4_MOVEMENT_SPEED_DATA_LEN);
            }
            b"ARMO" | b"WEAP" => {
                project_raw_array_rows(
                    record,
                    b"DAMA",
                    FO76_DAMAGE_TYPE_ROW_LEN,
                    FO4_DAMAGE_TYPE_ROW_LEN,
                );
            }
            _ => {}
        }
    }

    fn strip_term_looping_sound_snam(record: &mut Record) {
        if record.sig.0 != *b"TERM" {
            return;
        }

        // FO76 TERM reuses SNAM for a looping-sound formid; FO4's loader only
        // accepts TERM SNAM as marker-parameter rows and hard-crashes at
        // startup form load on any payload that is not a whole number of
        // rows. Sound links and empty subrecords have no FO4 representation,
        // so only decoded marker rows survive.
        record.fields.retain(|entry| {
            if entry.sig.0 != *b"SNAM" {
                return true;
            }
            match &entry.value {
                FieldValue::List(items) => !items.is_empty(),
                FieldValue::Struct(_) => true,
                FieldValue::Bytes(bytes) => {
                    !bytes.is_empty() && bytes.len() % FURNITURE_MARKER_PARAMETERS_ROW_LEN == 0
                }
                _ => false,
            }
        });
    }

    fn normalize_idlm_flags(record: &mut Record) {
        if record.sig.0 != *b"IDLM" {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 != *b"IDLF" {
                continue;
            }
            match &mut entry.value {
                FieldValue::Uint(value) => *value &= !u64::from(FO76_IDLM_UNKNOWN_5_FLAG),
                FieldValue::Int(value) => *value &= !i64::from(FO76_IDLM_UNKNOWN_5_FLAG),
                FieldValue::Bytes(bytes) if bytes.len() == 1 => {
                    bytes[0] &= !FO76_IDLM_UNKNOWN_5_FLAG;
                }
                _ => {}
            }
        }
    }

    fn clear_invalid_furniture_active_marker_bits(record: &mut Record) {
        if !matches!(&record.sig.0, b"FURN" | b"TERM") {
            return;
        }

        let mut marker_count = target_furniture_marker_count(record).min(22);
        // Interaction Point 0 is backed by the model's default furniture marker
        // whenever the record has a model, so it stays valid even when the record
        // carries no explicit marker subrecords. Many FO76 terminals rely on the
        // model marker with only `MNAM = InteractionPoint0 | HasModel` and no
        // SNAM/NAM0/ENAM rows; without this, `target_furniture_marker_count`
        // returns 0, Interaction Point 0 is cleared, and the terminal becomes
        // unusable in FO4 (no interaction point to activate).
        if marker_count == 0 && furniture_record_has_model(record) {
            marker_count = 1;
        }
        let valid_marker_bits = if marker_count == 0 {
            0
        } else {
            (1_u32 << marker_count) - 1
        };
        let invalid_marker_bits = FURNITURE_INTERACTION_POINT_BITS & !valid_marker_bits;
        if invalid_marker_bits == 0 {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 == *b"MNAM" {
                clear_u32_bits(&mut entry.value, invalid_marker_bits);
            }
        }
    }

    fn ensure_power_armor_furniture_vmad(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"FURN"
            || record.fields.iter().any(|entry| entry.sig.0 == *b"VMAD")
            || !record.fields.iter().any(|entry| {
                entry.sig.0 == *b"KWDA"
                    && fo4_keyword_value(&entry.value, interner, FO4_POWER_ARMOR_FURNITURE_KEYWORD)
            })
        {
            return;
        }

        record.fields.insert(
            0,
            FieldEntry {
                sig: SubrecordSig::from_str("VMAD").expect("VMAD is a valid signature"),
                value: FieldValue::Bytes(SmallVec::from_vec(power_armor_furniture_vmad_bytes())),
            },
        );
    }

    fn ensure_terminal_player_path_keyword(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"TERM"
            || !record.fields.iter().any(|entry| {
                entry.sig.0 == *b"MNAM"
                    && u32_field_has_bits(&entry.value, FURNITURE_INTERACTION_POINT_BITS)
            })
            || record.fields.iter().any(|entry| {
                entry.sig.0 == *b"KWDA"
                    && fo4_keyword_value(
                        &entry.value,
                        interner,
                        FO4_PLAYER_PATH_TO_FURNITURE_KEYWORD,
                    )
            })
        {
            return;
        }

        let keyword = FieldValue::FormKey(FormKey {
            local: FO4_PLAYER_PATH_TO_FURNITURE_KEYWORD,
            plugin: interner.intern(FO4_MASTER_NAME),
        });
        if let Some(entry) = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"KWDA")
        {
            match &mut entry.value {
                FieldValue::List(keywords) => keywords.push(keyword),
                FieldValue::Bytes(bytes) => {
                    bytes.extend_from_slice(&FO4_PLAYER_PATH_TO_FURNITURE_KEYWORD.to_le_bytes())
                }
                FieldValue::FormKey(_) => {
                    let existing = std::mem::replace(&mut entry.value, FieldValue::None);
                    entry.value = FieldValue::List(vec![existing, keyword]);
                }
                _ => return,
            }
        } else {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("KWDA").expect("KWDA is a valid signature"),
                value: FieldValue::List(vec![keyword]),
            });
        }

        Self::sync_keyword_count(record);
    }

    fn sync_keyword_count(record: &mut Record) {
        let count = record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"KWDA")
            .map(|entry| match &entry.value {
                FieldValue::List(keywords) => keywords.len() as u32,
                FieldValue::Bytes(bytes) => (bytes.len() / 4) as u32,
                FieldValue::FormKey(_) => 1,
                _ => 0,
            })
            .sum();

        if let Some(entry) = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"KSIZ")
        {
            set_u32_count(&mut entry.value, count);
            return;
        }

        let insert_at = record
            .fields
            .iter()
            .position(|entry| entry.sig.0 == *b"KWDA")
            .unwrap_or(record.fields.len());
        record.fields.insert(
            insert_at,
            FieldEntry {
                sig: SubrecordSig::from_str("KSIZ").expect("KSIZ is a valid signature"),
                value: FieldValue::Uint(u64::from(count)),
            },
        );
    }

    fn ensure_light_radius(interner: &crate::sym::StringInterner, record: &mut Record) {
        if record.sig.0 != *b"LIGH" {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 != *b"DATA" {
                continue;
            }
            match &mut entry.value {
                FieldValue::Bytes(bytes) => {
                    if bytes.len() < FO4_LIGH_DATA_RADIUS_OFFSET + 4 {
                        continue;
                    }
                    let radius = u32::from_le_bytes(
                        bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                            .try_into()
                            .unwrap(),
                    );
                    if radius > 0 {
                        continue;
                    }
                    let fallback = if bytes.len() >= FO76_LIGH_DATA_VALUE_OFFSET + 4 {
                        u32::from_le_bytes(
                            bytes[FO76_LIGH_DATA_VALUE_OFFSET..FO76_LIGH_DATA_VALUE_OFFSET + 4]
                                .try_into()
                                .unwrap(),
                        )
                    } else {
                        1
                    }
                    .clamp(1, FO4_LIGH_MAX_SYNTHETIC_RADIUS);
                    bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                        .copy_from_slice(&fallback.to_le_bytes());
                }
                FieldValue::Struct(fields) => {
                    let radius_index = fields.iter().position(|(name, _)| {
                        Self::struct_field_name_is(interner, *name, "Radius")
                    });
                    let fallback = Self::positive_u32_struct_field(interner, fields, "Value")
                        .unwrap_or(1)
                        .min(FO4_LIGH_MAX_SYNTHETIC_RADIUS);

                    if let Some(index) = radius_index {
                        if Self::field_value_positive_u32(&fields[index].1).is_none() {
                            fields[index].1 = FieldValue::Uint(u64::from(fallback));
                        }
                    } else {
                        fields.push((
                            interner.intern("Radius"),
                            FieldValue::Uint(u64::from(fallback)),
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    fn normalize_light_data_for_fo4(interner: &crate::sym::StringInterner, record: &mut Record) {
        if record.sig.0 != *b"LIGH" {
            return;
        }

        // Gobo/fire lights (a NAM0 mask is present) get FO4's tighter flicker ceiling.
        let max_flicker_intensity_amp = if Self::light_has_gobo(interner, record) {
            FO4_LIGH_MAX_FLICKER_INTENSITY_AMP_GOBO
        } else {
            FO4_LIGH_MAX_FLICKER_INTENSITY_AMP
        };

        for entry in &mut record.fields {
            if entry.sig.0 != *b"DATA" {
                continue;
            }
            match &mut entry.value {
                FieldValue::Bytes(bytes) => {
                    Self::normalize_raw_light_flags(bytes);
                    Self::raise_raw_light_near_clip(bytes, FO4_LIGH_MIN_NEAR_CLIP);
                    Self::clamp_raw_light_float_max(
                        bytes,
                        FO4_LIGH_DATA_FLICKER_INTENSITY_AMP_OFFSET,
                        max_flicker_intensity_amp,
                    );
                    if bytes.len() >= FO4_LIGH_DATA_SCALAR_OFFSET + 4 {
                        bytes[FO4_LIGH_DATA_SCALAR_OFFSET..FO4_LIGH_DATA_SCALAR_OFFSET + 4]
                            .copy_from_slice(&FO4_LIGH_DEFAULT_SCALAR.to_le_bytes());
                    }
                    if bytes.len() >= FO4_LIGH_DATA_EXPONENT_OFFSET + 4 {
                        bytes[FO4_LIGH_DATA_EXPONENT_OFFSET..FO4_LIGH_DATA_EXPONENT_OFFSET + 4]
                            .copy_from_slice(&FO4_LIGH_DEFAULT_EXPONENT.to_le_bytes());
                    }
                    if bytes.len() >= FO4_LIGH_DATA_VALUE_OFFSET + 4 {
                        bytes[FO4_LIGH_DATA_VALUE_OFFSET..FO4_LIGH_DATA_VALUE_OFFSET + 4]
                            .copy_from_slice(&FO4_LIGH_DEFAULT_VALUE.to_le_bytes());
                    }
                    if bytes.len() >= FO4_LIGH_DATA_WEIGHT_OFFSET + 4 {
                        bytes[FO4_LIGH_DATA_WEIGHT_OFFSET..FO4_LIGH_DATA_WEIGHT_OFFSET + 4]
                            .copy_from_slice(&FO4_LIGH_DEFAULT_WEIGHT.to_le_bytes());
                    }
                    if bytes.len() > FO4_LIGH_DATA_LEN {
                        bytes.truncate(FO4_LIGH_DATA_LEN);
                    }
                }
                FieldValue::Struct(fields) => {
                    fields.retain(|(name, _)| {
                        !Self::struct_field_name_is(interner, *name, "Value")
                            && !Self::struct_field_name_is(interner, *name, "Bytes19")
                            && !Self::struct_field_name_is(interner, *name, "Weight")
                    });
                    Self::ensure_struct_float_field(
                        interner,
                        fields,
                        "Scalar",
                        FO4_LIGH_DEFAULT_SCALAR,
                    );
                    Self::ensure_struct_float_field(
                        interner,
                        fields,
                        "Exponent",
                        FO4_LIGH_DEFAULT_EXPONENT,
                    );
                    Self::clamp_struct_float_field(
                        interner,
                        fields,
                        "FlickerEffectIntensityAmplitude",
                        max_flicker_intensity_amp,
                    );
                    Self::raise_struct_float_field(
                        interner,
                        fields,
                        "NearClip",
                        FO4_LIGH_MIN_NEAR_CLIP,
                    );
                    Self::normalize_struct_light_flags(interner, fields);
                }
                _ => {}
            }
        }
    }

    /// Whether a LIGH carries a projected-light mask (`NAM0` gobo) — the signal
    /// used to pick FO4's tighter flicker ceiling for gobo/fire lights.
    fn light_has_gobo(interner: &crate::sym::StringInterner, record: &Record) -> bool {
        record.fields.iter().any(|entry| {
            if entry.sig.0 != *b"NAM0" {
                return false;
            }
            match &entry.value {
                FieldValue::String(sym) => interner
                    .resolve(*sym)
                    .is_some_and(|path| !path.trim().is_empty()),
                FieldValue::Bytes(bytes) => !trim_nul_suffix(bytes.as_slice()).is_empty(),
                _ => false,
            }
        })
    }

    /// Clear FO76's `non_specular` bit and drop FO76-only high flag bits from a raw
    /// `DATA` blob so the converted light uses FO4-native specular and valid flags.
    fn normalize_raw_light_flags(bytes: &mut [u8]) {
        if bytes.len() < FO4_LIGH_DATA_FLAGS_OFFSET + 4 {
            return;
        }
        let range = FO4_LIGH_DATA_FLAGS_OFFSET..FO4_LIGH_DATA_FLAGS_OFFSET + 4;
        let flags = u32::from_le_bytes(bytes[range.clone()].try_into().unwrap());
        let normalized = (flags & FO4_LIGH_FLAGS_VALID_MASK) & !FO4_LIGH_FLAGS_NON_SPECULAR;
        bytes[range].copy_from_slice(&normalized.to_le_bytes());
    }

    /// Clamp a raw `DATA` float field down to `maximum` (also normalizes a
    /// non-finite value to `maximum`). Mirror of `raise_raw_light_near_clip`.
    fn clamp_raw_light_float_max(bytes: &mut [u8], offset: usize, maximum: f32) {
        if bytes.len() < offset + 4 {
            return;
        }
        let current = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        if current.is_finite() && current <= maximum {
            return;
        }
        bytes[offset..offset + 4].copy_from_slice(&maximum.to_le_bytes());
    }

    /// Struct-branch counterpart of `clamp_raw_light_float_max`. Leaves an absent
    /// field alone (a missing flicker amplitude means the field simply isn't there).
    fn clamp_struct_float_field(
        interner: &crate::sym::StringInterner,
        fields: &mut [(crate::sym::Sym, FieldValue)],
        field_name: &str,
        maximum: f32,
    ) {
        let Some(index) = fields
            .iter()
            .position(|(name, _)| Self::struct_field_name_is(interner, *name, field_name))
        else {
            return;
        };
        let needs_clamp = match &fields[index].1 {
            FieldValue::Float(value) => !value.is_finite() || *value > maximum,
            _ => false,
        };
        if needs_clamp {
            fields[index].1 = FieldValue::Float(maximum);
        }
    }

    /// Struct-branch counterpart of `normalize_raw_light_flags`, best-effort for an
    /// integer-valued `Flags` field; other representations are left untouched.
    fn normalize_struct_light_flags(
        interner: &crate::sym::StringInterner,
        fields: &mut [(crate::sym::Sym, FieldValue)],
    ) {
        for (name, value) in fields.iter_mut() {
            if !Self::struct_field_name_is(interner, *name, "Flags") {
                continue;
            }
            let current = match value {
                FieldValue::Uint(v) => u32::try_from(*v).ok(),
                FieldValue::Int(v) => u32::try_from(*v).ok(),
                _ => None,
            };
            if let Some(flags) = current {
                let normalized = (flags & FO4_LIGH_FLAGS_VALID_MASK) & !FO4_LIGH_FLAGS_NON_SPECULAR;
                *value = FieldValue::Uint(u64::from(normalized));
            }
        }
    }

    fn normalize_cage_bulb_gobo_light_for_fo4(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"LIGH" || !Self::record_has_cage_bulb_gobo(interner, record) {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 != *b"DATA" {
                continue;
            }
            match &mut entry.value {
                FieldValue::Bytes(bytes) => {
                    if bytes.len() < FO4_LIGH_DATA_RADIUS_OFFSET + 4 {
                        continue;
                    }
                    let radius = u32::from_le_bytes(
                        bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                            .try_into()
                            .unwrap(),
                    );
                    if radius <= FO4_CAGE_BULB_GOBO_MAX_RADIUS {
                        continue;
                    }
                    bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                        .copy_from_slice(&FO4_CAGE_BULB_GOBO_MAX_RADIUS.to_le_bytes());
                    Self::raise_raw_light_near_clip(bytes, FO4_CAGE_BULB_GOBO_MIN_NEAR_CLIP);
                }
                FieldValue::Struct(fields) => {
                    let Some(radius_index) = fields.iter().position(|(name, _)| {
                        Self::struct_field_name_is(interner, *name, "Radius")
                    }) else {
                        continue;
                    };
                    let Some(radius) = Self::field_value_positive_u32(&fields[radius_index].1)
                    else {
                        continue;
                    };
                    if radius <= FO4_CAGE_BULB_GOBO_MAX_RADIUS {
                        continue;
                    }
                    fields[radius_index].1 =
                        FieldValue::Uint(u64::from(FO4_CAGE_BULB_GOBO_MAX_RADIUS));
                    Self::raise_struct_float_field(
                        interner,
                        fields,
                        "NearClip",
                        FO4_CAGE_BULB_GOBO_MIN_NEAR_CLIP,
                    );
                }
                _ => {}
            }
        }
    }

    fn record_has_cage_bulb_gobo(interner: &crate::sym::StringInterner, record: &Record) -> bool {
        record.fields.iter().any(|entry| {
            if entry.sig.0 != *b"NAM0" {
                return false;
            }
            match &entry.value {
                FieldValue::String(sym) => interner
                    .resolve(*sym)
                    .is_some_and(Self::is_cage_bulb_gobo_path),
                FieldValue::Bytes(bytes) => std::str::from_utf8(trim_nul_suffix(bytes.as_slice()))
                    .ok()
                    .is_some_and(Self::is_cage_bulb_gobo_path),
                _ => false,
            }
        })
    }

    fn is_cage_bulb_gobo_path(path: &str) -> bool {
        let normalized = path.replace('\\', "/").to_ascii_lowercase();
        let without_data = normalized
            .strip_prefix("data/")
            .unwrap_or(normalized.as_str());
        without_data == FO4_CAGE_BULB_GOBO_PATH
    }

    fn raise_raw_light_near_clip(bytes: &mut [u8], minimum: f32) {
        if bytes.len() < FO4_LIGH_DATA_NEAR_CLIP_OFFSET + 4 {
            return;
        }
        let current = f32::from_le_bytes(
            bytes[FO4_LIGH_DATA_NEAR_CLIP_OFFSET..FO4_LIGH_DATA_NEAR_CLIP_OFFSET + 4]
                .try_into()
                .unwrap(),
        );
        if current.is_finite() && current >= minimum {
            return;
        }
        bytes[FO4_LIGH_DATA_NEAR_CLIP_OFFSET..FO4_LIGH_DATA_NEAR_CLIP_OFFSET + 4]
            .copy_from_slice(&minimum.to_le_bytes());
    }

    fn ensure_light_fade_value(record: &mut Record) {
        if record.sig.0 != *b"LIGH" || record.fields.iter().any(|entry| entry.sig.0 == *b"FNAM") {
            return;
        }

        let fade = FieldEntry {
            sig: SubrecordSig(*b"FNAM"),
            value: FieldValue::Float(FO4_LIGH_DEFAULT_FADE),
        };
        if let Some(data_index) = record
            .fields
            .iter()
            .position(|entry| entry.sig.0 == *b"DATA")
        {
            record.fields.insert(data_index + 1, fade);
        } else {
            record.fields.push(fade);
        }
    }

    fn ensure_struct_float_field(
        interner: &crate::sym::StringInterner,
        fields: &mut Vec<(crate::sym::Sym, FieldValue)>,
        field_name: &str,
        default: f32,
    ) {
        if fields
            .iter()
            .any(|(name, _)| Self::struct_field_name_is(interner, *name, field_name))
        {
            return;
        }

        fields.push((interner.intern(field_name), FieldValue::Float(default)));
    }

    fn raise_struct_float_field(
        interner: &crate::sym::StringInterner,
        fields: &mut Vec<(crate::sym::Sym, FieldValue)>,
        field_name: &str,
        minimum: f32,
    ) {
        let Some(index) = fields
            .iter()
            .position(|(name, _)| Self::struct_field_name_is(interner, *name, field_name))
        else {
            fields.push((interner.intern(field_name), FieldValue::Float(minimum)));
            return;
        };

        let needs_raise = match &fields[index].1 {
            FieldValue::Float(value) => !value.is_finite() || *value < minimum,
            _ => true,
        };
        if needs_raise {
            fields[index].1 = FieldValue::Float(minimum);
        }
    }

    fn struct_field_name_is(
        interner: &crate::sym::StringInterner,
        name: crate::sym::Sym,
        expected: &str,
    ) -> bool {
        interner.resolve(name).is_some_and(|actual| {
            actual.eq_ignore_ascii_case(expected)
                || actual.replace('_', "").eq_ignore_ascii_case(expected)
        })
    }

    fn positive_u32_struct_field(
        interner: &crate::sym::StringInterner,
        fields: &[(crate::sym::Sym, FieldValue)],
        field_name: &str,
    ) -> Option<u32> {
        fields
            .iter()
            .find(|(name, _)| Self::struct_field_name_is(interner, *name, field_name))
            .and_then(|(_, value)| Self::field_value_positive_u32(value))
    }

    fn field_value_positive_u32(value: &FieldValue) -> Option<u32> {
        match value {
            FieldValue::Uint(value) => u32::try_from(*value).ok().filter(|value| *value > 0),
            FieldValue::Int(value) => u32::try_from(*value).ok().filter(|value| *value > 0),
            FieldValue::Float(value) if *value > 0.0 => Some(*value as u32),
            _ => None,
        }
    }

    fn convert_fo76_leveled_list_entries(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if !matches!(&record.sig.0, b"LVLI" | b"LVLN" | b"LVSP") {
            return;
        }

        let old_fields: Vec<FieldEntry> = record.fields.drain(..).collect();
        let mut retained = smallvec::SmallVec::new();
        let mut converted_count = 0usize;
        let mut saw_lvlo = false;

        for index in 0..old_fields.len() {
            let mut entry = old_fields[index].clone();
            if entry.sig.0 == *b"LVLO" {
                saw_lvlo = true;
                if Self::leveled_entry_gated_by_dropped_world_state(&old_fields, index) {
                    // FO76 gates this entry behind nuke-zone / event-global world
                    // state that never occurs in FO4. FO4 LVLI entries can't carry
                    // per-entry conditions, so retaining it would leak the
                    // special-state variant (nuke-zone radiation suits, holiday
                    // outfits) into normal gameplay.
                    continue;
                }
                if let Some(reference) =
                    source_lvlo_reference(&entry.value, record.form_key.plugin, interner)
                {
                    let reference = remap_known_lvlo_reference(interner, reference);
                    let level = raw_lvlo_u16(&entry.value, 0)
                        .unwrap_or_else(|| following_u16_value(&old_fields, index + 1, b"LVLV", 1));
                    let count = raw_lvlo_u16(&entry.value, 8)
                        .unwrap_or_else(|| following_u16_value(&old_fields, index + 1, b"LVIV", 1));
                    entry.value = fo4_lvlo_value(interner, &record.sig.0, level, reference, count);
                    converted_count += 1;
                    retained.push(entry);
                }
                continue;
            }
            retained.push(entry);
        }

        record.fields = retained;
        if saw_lvlo {
            sync_llct_count(record, converted_count);
        }
    }

    fn raw_condition_function_id(bytes: &[u8]) -> Option<u16> {
        if bytes.len() < 10 {
            return None;
        }
        Some(u16::from_le_bytes([bytes[8], bytes[9]]))
    }

    fn set_raw_condition_function_id(bytes: &mut [u8], function_id: u16) {
        if bytes.len() >= 10 {
            bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        }
    }

    fn raw_condition_parameter_1(bytes: &[u8]) -> Option<u32> {
        if bytes.len() < 16 {
            return None;
        }
        Some(u32::from_le_bytes([
            bytes[12], bytes[13], bytes[14], bytes[15],
        ]))
    }

    fn condition_function_id(
        interner: &crate::sym::StringInterner,
        value: &FieldValue,
    ) -> Option<u16> {
        match value {
            FieldValue::Bytes(bytes) => Self::raw_condition_function_id(bytes.as_slice()),
            FieldValue::Struct(fields) => {
                named_value_canonical(fields, "Function", interner).and_then(field_value_to_u16)
            }
            _ => None,
        }
    }

    fn condition_parameter_1(
        interner: &crate::sym::StringInterner,
        value: &FieldValue,
    ) -> Option<u32> {
        match value {
            FieldValue::Bytes(bytes) => Self::raw_condition_parameter_1(bytes.as_slice()),
            FieldValue::Struct(fields) => {
                named_value_canonical(fields, "Parameter1", interner).and_then(field_value_to_u32)
            }
            _ => None,
        }
    }

    fn raw_condition_run_on(bytes: &[u8]) -> Option<u32> {
        if bytes.len() < 24 {
            return None;
        }
        Some(u32::from_le_bytes([
            bytes[20], bytes[21], bytes[22], bytes[23],
        ]))
    }

    fn condition_run_on(interner: &crate::sym::StringInterner, value: &FieldValue) -> Option<u32> {
        match value {
            FieldValue::Bytes(bytes) => Self::raw_condition_run_on(bytes.as_slice()),
            FieldValue::Struct(fields) => {
                named_value_canonical(fields, "RunOn", interner).and_then(field_value_to_u32)
            }
            _ => None,
        }
    }

    /// CTDA comparison operator — the high 3 bits of the type byte (0=Equal,
    /// 1=NotEqual, 2=Greater, 3=GreaterOrEqual, 4=Less, 5=LessOrEqual).
    fn raw_condition_operator(bytes: &[u8]) -> Option<u8> {
        bytes.first().map(|b| (b >> 5) & 0x07)
    }

    /// CTDA comparison value (f32 at bytes [4..8]).
    fn raw_condition_comparison_value(bytes: &[u8]) -> Option<f32> {
        if bytes.len() < 8 {
            return None;
        }
        Some(f32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]))
    }

    /// True when a leveled-list `LVLO` at `lvlo_index` is gated by a trailing
    /// `CTDA` describing FO76-only world state (a nuke-zone check or an event
    /// global that is ON). Scans the entry's conditions up to the next `LVLO`.
    fn leveled_entry_gated_by_dropped_world_state(
        old_fields: &[FieldEntry],
        lvlo_index: usize,
    ) -> bool {
        old_fields[lvlo_index + 1..]
            .iter()
            .take_while(|entry| entry.sig.0 != *b"LVLO")
            .filter(|entry| matches!(&entry.sig.0, b"CTDA" | b"CTDT"))
            .any(|entry| Self::condition_gates_dropped_world_state(&entry.value))
    }

    /// True when a `CTDA` gates its entry behind FO76-only world state that never
    /// obtains in FO4: a nuke-zone check (func 849), or a `GetGlobalValue` gate
    /// (func 74) requiring an event/seasonal global to be ON.
    fn condition_gates_dropped_world_state(value: &FieldValue) -> bool {
        let FieldValue::Bytes(bytes) = value else {
            return false;
        };
        let bytes = bytes.as_slice();
        match Self::raw_condition_function_id(bytes) {
            Some(FO76_NUKE_ZONE_CONDITION_FUNCTION_ID) => true,
            Some(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID) => {
                Self::condition_requires_global_on(bytes)
            }
            _ => false,
        }
    }

    /// True when a `GetGlobalValue` condition can only be satisfied while the
    /// global is non-zero (event ON). The off-state (global 0, the FO4 default)
    /// branch is kept so the normal-world entry survives.
    fn condition_requires_global_on(bytes: &[u8]) -> bool {
        let (Some(op), Some(cmp)) = (
            Self::raw_condition_operator(bytes),
            Self::raw_condition_comparison_value(bytes),
        ) else {
            return false;
        };
        match op {
            0 => cmp != 0.0, // == a non-zero value
            1 => cmp == 0.0, // != zero
            2 => cmp >= 0.0, // > a value the off-state (0) cannot exceed
            3 => cmp > 0.0,  // >= a positive value
            _ => false,      // less-than variants: global 0 satisfies -> keep
        }
    }

    /// Returns `true` if this record type synthesizes its `Effects` group.
    ///
    /// Called by the orchestrator before field dispatch to decide whether to
    /// decode the Effects subrecords from the source ESP or synthesize them.
    pub fn is_effects_synthetic(record_sig: SigCode) -> bool {
        EFFECTS_SYNTHETIC_RECORD_SIGS
            .iter()
            .any(|sig| record_sig.0 == *sig)
    }

    /// Returns the effects key rerouting for the given record type and field,
    /// or `None` if no rerouting is needed.
    ///
    /// Mirrors `Fo76ToFo4Hooks::translate_effects_keys`.
    ///
    /// For ALCH/ENCH/SPEL: DATA/EFID/EffectData → Effects::EFID
    /// For PERK: DATA → Effects::DATA
    pub fn translate_effects_key(
        record_sig: SigCode,
        field_sig: SubrecordSig,
    ) -> Option<EffectsKeyRoute> {
        // Only applies when the record has an Effects group.
        if !Self::is_effects_synthetic(record_sig) {
            return None;
        }
        match &record_sig.0 {
            b"ALCH" | b"ENCH" | b"SPEL" => {
                // DATA, EFID, EFIT (EffectData) → Effects / EFID
                match &field_sig.0 {
                    b"DATA" | b"EFID" | b"EFIT" => Some(EffectsKeyRoute {
                        field_sig,
                        target_sig: SubrecordSig(*b"EFID"),
                    }),
                    _ => None,
                }
            }
            b"PERK" => {
                // DATA → Effects / DATA
                match &field_sig.0 {
                    b"DATA" => Some(EffectsKeyRoute {
                        field_sig,
                        target_sig: SubrecordSig(*b"DATA"),
                    }),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

// FO76 QUST.DATA layout variants (the `flags` field is a union on
// `record_form_version`; detected here by payload length). FO4 QUST.DNAM is a
// fixed 12-byte `struct:H,B,B,f,B,B,B,B`.
const FO76_QUST_DATA_FLAGS64_LEN: usize = 20; // flags u64 (form_version >= 202)
const FO76_QUST_DATA_FLAGS32_LEN: usize = 16; // flags u32 (form_version < 202)
const FO4_QUST_DNAM_LEN: usize = 12;
const FO76_QUST_TYPE_PUBLIC_EVENT: u8 = 6;
const FO76_QUST_TYPE_EVENT: u8 = 8;

/// FO76 activity quests such as `TW043` use the Event quest type. Neither they
/// nor Public Events may start with the game in FO4; Story Manager can still
/// start their carried nodes later.
pub(crate) fn fo76_qust_type_disables_start_game(quest_type: u8) -> bool {
    matches!(
        quest_type,
        FO76_QUST_TYPE_PUBLIC_EVENT | FO76_QUST_TYPE_EVENT
    )
}

/// Relayout an FO76 QUST `DATA` payload into an FO4 QUST `DNAM` payload.
///
/// The low 16 flag bits are bit-identical between the two games
/// (`start_game_enabled`=1, `starts_enabled`=16, `run_once`=256,
/// `has_dialogue_data`=0x8000, …); FO76-only flag bits (>= 0x10000) are dropped
/// by the u16 truncation. `priority`, `delay_time`, and `quest_type` carry over
/// at their FO4 offsets. Returns `None` for an unrecognized length so the caller
/// leaves the field untouched.
fn build_fo4_qust_dnam_from_fo76_data(data: &[u8]) -> Option<smallvec::SmallVec<[u8; 32]>> {
    let (flags, priority, delay_time, quest_type) = match data.len() {
        FO76_QUST_DATA_FLAGS64_LEN => (
            u64::from_le_bytes(data[0..8].try_into().ok()?) as u16,
            data[8],
            [data[12], data[13], data[14], data[15]],
            data[16],
        ),
        FO76_QUST_DATA_FLAGS32_LEN => (
            u32::from_le_bytes(data[0..4].try_into().ok()?) as u16,
            data[4],
            [data[8], data[9], data[10], data[11]],
            data[12],
        ),
        _ => return None,
    };
    let mut dnam: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
    dnam.resize(FO4_QUST_DNAM_LEN, 0);
    dnam[0..2].copy_from_slice(&flags.to_le_bytes());
    dnam[2] = priority;
    dnam[4..8].copy_from_slice(&delay_time);
    dnam[8] = quest_type;
    Some(dnam)
}

// FO4 QUST.DNAM flag bits (low u16 of the flags field).
const QUST_DNAM_FLAG_START_GAME_ENABLED: u16 = 0x0001;
const QUST_DNAM_FLAG_HAS_DIALOGUE_DATA: u16 = 0x8000;

/// True when the QUST's editorID marks it as an NPC conversation quest. FO76
/// names these `Dialogue_*` / `*_Dialogue_*` / `W05_Dialogue*` /
/// `XPD_Dialogue_*` / `NPCConversation_*`, while gameplay quests are named by
/// quest-type prefix (`RE_`, `*_MQ_`, `MTR*`, `FF*`, `EN*`, `Test*`). Naming is
/// the only signal that cleanly separates the two; every structural signal also
/// matches random encounters / events.
fn qust_eid_lower(interner: &crate::sym::StringInterner, record: &Record) -> Option<String> {
    if let Some(eid) = record.eid.and_then(|sym| interner.resolve(sym)) {
        return Some(eid.to_ascii_lowercase());
    }
    for entry in &record.fields {
        if entry.sig.0 != *b"EDID" {
            continue;
        }
        return match &entry.value {
            FieldValue::String(sym) => interner.resolve(*sym).map(|s| s.to_ascii_lowercase()),
            FieldValue::Bytes(bytes) => {
                let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                std::str::from_utf8(&bytes[..end])
                    .ok()
                    .map(|s| s.to_ascii_lowercase())
            }
            _ => None,
        };
    }
    None
}

pub(crate) fn qust_eid_is_dialogue_conversation(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> bool {
    qust_eid_lower(interner, record)
        .is_some_and(|s| s.contains("dialogue") || s.contains("npcconversation"))
}

/// FO76 ships hundreds of developer test/scratch quests (EditorID `Test*`,
/// `zz*`/`ZZZ*`) whose scenes bind aliases to test-only world content. They are
/// never started for players in FO76 — its Story Manager / test harness drives
/// them, and that machinery is not replicated here. Auto-starting one (whether
/// via force-start or a faithfully-relayed start-game-enabled flag) makes FO4
/// try to fill those aliases, resolve a bad actor handle, and CTD on load
/// (`test_VHarbison_Dialogue_Someone`). Treat them as never-auto-run.
fn qust_eid_is_test_or_dev(interner: &crate::sym::StringInterner, record: &Record) -> bool {
    qust_eid_lower(interner, record).is_some_and(|s| s.starts_with("test") || s.starts_with("zz"))
}

/// True when the QUST's editorID marks it as a radio-station quest (`*Radio*`).
/// Radio stations are kept enabled alongside dialogue quests; their FO76
/// start-game-enabled flag is preserved as-is (real stations ship it set;
/// main-quest "radio" segments don't, so they remain off).
fn qust_eid_is_radio(interner: &crate::sym::StringInterner, record: &Record) -> bool {
    qust_eid_lower(interner, record)
        .is_some_and(|s| s.contains("radio") && s != "cb_highschoolpasystem_radioscenes")
}

/// True when the QUST's editorID marks it as a dedicated holotape-scene
/// container. FO4 keeps every holotape's scene under an always-running quest
/// (`HolotapesQuest`, `DLC04Holotapes`); FO76 mirrors this with per-holotape
/// containers (`HolotapeQuest_*`, `*_HolotapeQuest`, `*_Holotapes_*`). A Voice
/// holotape plays by running its scene, and a scene only runs while its owning
/// quest is running, so the container must be start-game-enabled or the holotape
/// is silent. Gameplay/main quests that merely involve a holotape carry a bare
/// `*Holotape` suffix (`..._DanHolotape`, `..._Holotape_Misc`) — they lack the
/// `holotapequest`/`holotapes` marker and stay excluded, so this does not
/// re-enable actor-seizing story quests.
fn qust_eid_is_holotape(interner: &crate::sym::StringInterner, record: &Record) -> bool {
    qust_eid_lower(interner, record)
        .is_some_and(|s| s.contains("holotapequest") || s.contains("holotapes"))
}

/// Force `start_game_enabled` on a freshly built FO4 QUST DNAM so the dialogue
/// quest auto-starts (FO76 started it via the now-dropped Story Manager). Guarded
/// on `has_dialogue_data` to skip degenerate non-dialogue quests. Idempotent.
fn force_dialogue_quest_autostart(dnam: &mut [u8]) {
    if dnam.len() < 2 {
        return;
    }
    let mut flags = u16::from_le_bytes([dnam[0], dnam[1]]);
    if flags & QUST_DNAM_FLAG_HAS_DIALOGUE_DATA == 0 {
        return;
    }
    flags |= QUST_DNAM_FLAG_START_GAME_ENABLED;
    dnam[0..2].copy_from_slice(&flags.to_le_bytes());
}

/// Clear `start_game_enabled` so a developer test/scratch quest never
/// auto-starts in the converted game (see [`qust_eid_is_test_or_dev`]).
fn suppress_quest_autostart(dnam: &mut [u8]) {
    if dnam.len() < 2 {
        return;
    }
    let mut flags = u16::from_le_bytes([dnam[0], dnam[1]]);
    flags &= !QUST_DNAM_FLAG_START_GAME_ENABLED;
    dnam[0..2].copy_from_slice(&flags.to_le_bytes());
}

fn fo76_dial_category_to_fo4(value: u8) -> u8 {
    match value {
        FO76_DIAL_CATEGORY_DETECTION => FO4_DIAL_CATEGORY_DETECTION,
        FO76_DIAL_CATEGORY_MISCELLANEOUS => FO4_DIAL_CATEGORY_MISCELLANEOUS,
        _ => value,
    }
}

fn fo76_map_marker_type_to_fo4(source_type: u16) -> u8 {
    match source_type {
        // Shared icon names shift at the FO4-only Diamond City, Bunker Hill,
        // Faneuil Hall, Synth Head, and Prydwen enum slots.
        0..=1 => source_type as u8,
        2..=15 => (source_type + 1) as u8,
        16..=22 => (source_type + 2) as u8,
        23..=44 => (source_type + 3) as u8,
        45..=54 => (source_type + 4) as u8,
        55..=63 => (source_type + 5) as u8,
        64 => 6,
        65 => 4,
        66 => 8,
        67..=70 => 15,
        71 => 74,
        72 => 22,
        73..=74 => 4,
        75 => 21,
        76 => 56,
        77 => 77,
        78 => 26,
        79..=80 => 18,
        81 => 37,
        82 => 8,
        83..=84 => 26,
        85 => 18,
        86 => 61,
        87 => 5,
        88 => 9,
        89 => 8,
        90 => 40,
        91 => 62,
        92 => 22,
        93 => 54,
        94 => 4,
        95 => 73,
        96 => 21,
        97 => 69,
        98 => 8,
        99 => 13,
        109 => 13,
        111 => 8,
        112 => 41,
        _ => 77,
    }
}

fn normalize_u8_field_value(value: &mut FieldValue, map: fn(u8) -> u8) {
    match value {
        FieldValue::Uint(n) if *n <= u64::from(u8::MAX) => *n = u64::from(map(*n as u8)),
        FieldValue::Int(n) if (0..=i64::from(u8::MAX)).contains(n) => {
            *n = i64::from(map(*n as u8));
        }
        FieldValue::Bytes(bytes) if !bytes.is_empty() => bytes[0] = map(bytes[0]),
        _ => {}
    }
}

fn scol_onam_is_usable(value: &FieldValue) -> bool {
    match value {
        FieldValue::FormKey(form_key) => form_key.local & 0x00FF_FFFF != 0,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            u32::from_le_bytes(bytes[..4].try_into().expect("four-byte ONAM prefix")) & 0x00FF_FFFF
                != 0
        }
        FieldValue::Uint(value) => *value & 0x00FF_FFFF != 0,
        FieldValue::Int(value) => *value > 0 && (*value as u64) & 0x00FF_FFFF != 0,
        FieldValue::List(values) => values.iter().any(scol_onam_is_usable),
        FieldValue::Struct(fields) => fields.iter().any(|(_, value)| scol_onam_is_usable(value)),
        _ => false,
    }
}

fn field_value_has_non_empty_text(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
) -> bool {
    match value {
        FieldValue::String(value) => interner
            .resolve(*value)
            .is_some_and(|value| !value.trim_matches(['\0', ' ', '\t', '\r', '\n']).is_empty()),
        FieldValue::Bytes(bytes) => bytes
            .split(|byte| *byte == 0)
            .next()
            .is_some_and(|value| value.iter().any(|byte| !byte.is_ascii_whitespace())),
        FieldValue::List(values) => values
            .iter()
            .any(|value| field_value_has_non_empty_text(value, interner)),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| field_value_has_non_empty_text(value, interner)),
        _ => false,
    }
}

impl PairHook for Fo76Fo4Hook {
    /// Drop FO76-only global fields before field translation begins.
    fn pre_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        Self::normalize_arma_upper_body_skin_slots(record);
        Self::normalize_chinese_stealth_arma_pipboy_slot(ctx.interner, record);
        Self::drop_global_fields(record);
        crate::target_normalize::strip_unsupported_race_tint_tables(record);
        Self::convert_nif_backed_empty_scol_to_stat(ctx.interner, record);
        Self::strip_wrld_runtime_tables(record);
        Self::strip_redundant_omod_target_keywords(ctx.interner, record);
        Self::strip_tesla_cannon_receiver_model(ctx.interner, record);
        Self::strip_material_omod_models(ctx.interner, record);
        Self::normalize_npc_perk_entries(ctx.interner, record);
        Self::normalize_npc_raw_form_refs(ctx.interner, record);
        Self::normalize_cont_raw_form_refs(ctx.interner, record);
        Self::strip_info_editor_id(record);
        Self::strip_orphan_term_conditions(record);
        Self::normalize_refr_map_marker_tnam(record);
        Self::rename_furniture_marker_parameters(record);
        Self::strip_zero_health_cont_destructibles(ctx.interner, record);
        Self::normalize_note_scene_ref(record);
        Self::convert_mgef_data_to_fo4_layout(record);
        Self::normalize_scen_player_dialogue_choices(record);
        Self::normalize_dial_data_category(ctx.interner, record);
        Self::strip_scorched_statue_activation_conditions(ctx.interner, record);
        Self::convert_fo76_leveled_list_entries(ctx.interner, record);
        Self::convert_or_drop_cell_combined_reference_index(ctx.interner, record);
        Self::convert_qust_data_to_fo4_dnam(ctx.interner, record);
        Self::strip_qust_runtime_scopes(record);
        Ok(())
    }

    fn post_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        Self::namespace_radio_receiver_frequency(ctx.interner, record);
        Self::strip_pack_runtime_refs(record);
        Self::neutralize_dangling_package_alias_targets(record);
        Self::drop_fo4_incompatible_conditions(ctx.interner, record);
        Self::normalize_workshop_cobj(ctx.interner, record);
        Self::convert_mgef_data_to_fo4_layout(record);
        Self::drop_perk_vmad(record);
        Self::drop_mstt_omod_data(ctx.interner, record);
        Self::strip_invalid_object_mod_properties(ctx.interner, record);
        Self::normalize_omod_material_swap_functions(ctx.interner, record);
        Self::normalize_npc_raw_form_refs(ctx.interner, record);
        Self::normalize_rd01_assassin_combat_style(ctx.interner, record);
        Self::convert_or_drop_region_objects(ctx.interner, record);
        Self::map_fo76_fallback_package_procedure(ctx.interner, record);
        Self::normalize_fo76_pack_procedure_tree(ctx.interner, record);
        Self::strip_fo76_only_subrecord_tails(record);
        Self::normalize_idlm_flags(record);
        Self::ensure_power_armor_furniture_vmad(ctx.interner, record);
        Self::strip_term_looping_sound_snam(record);
        Self::clear_invalid_furniture_active_marker_bits(record);
        Self::ensure_terminal_player_path_keyword(ctx.interner, record);
        Self::ensure_light_radius(ctx.interner, record);
        Self::normalize_light_data_for_fo4(ctx.interner, record);
        Self::normalize_cage_bulb_gobo_light_for_fo4(ctx.interner, record);
        Self::ensure_light_fade_value(record);
        model_paths::normalize_model_paths(ctx.interner, record);
        Ok(())
    }

    /// No synthetic records produced by this pair.
    fn synthesize_records(&self, _ctx: &mut PairCtx<'_>) -> Vec<Record> {
        Vec::new()
    }
}

fn fo4_keyword_value(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
    keyword: u32,
) -> bool {
    match value {
        FieldValue::FormKey(form_key) => {
            form_key.local == keyword
                && interner
                    .resolve(form_key.plugin)
                    .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO4_MASTER_NAME))
        }
        FieldValue::List(values) => values
            .iter()
            .any(|value| fo4_keyword_value(value, interner, keyword)),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| fo4_keyword_value(value, interner, keyword)),
        FieldValue::Bytes(bytes) => bytes.chunks_exact(4).any(|bytes| {
            u32::from_le_bytes(bytes.try_into().expect("four-byte FormID row")) & 0x00FF_FFFF
                == keyword
        }),
        _ => false,
    }
}

fn power_armor_furniture_vmad_bytes() -> Vec<u8> {
    let masters = [FO4_MASTER_NAME.to_string()];
    let payload = serde_json::json!({
        "Version": FO4_VMAD_VERSION,
        "Object Format": FO4_VMAD_OBJECT_FORMAT,
        "Scripts": [{
            "ScriptName": POWER_ARMOR_BATTERY_INSERT_SCRIPT,
            "Properties": [
                power_armor_vmad_object_property("firstPersonKW", FO4_POWER_ARMOR_FIRST_PERSON_KEYWORD),
                power_armor_vmad_object_property("batteryInsertAnimKW", FO4_POWER_ARMOR_BATTERY_INSERT_ANIM_KEYWORD),
                power_armor_vmad_object_property("PlayerPathToFurniture", FO4_PLAYER_PATH_TO_FURNITURE_KEYWORD),
                power_armor_vmad_object_property("batteryItemKW", FO4_POWER_ARMOR_BATTERY_ITEM_KEYWORD),
                power_armor_vmad_object_property("powerArmorFurnitureKW", FO4_POWER_ARMOR_FURNITURE_KEYWORD),
            ],
        }],
    });
    build_vmad_bytes_from_payload(&payload, &masters, FO76_MASTER_NAME)
        .expect("power armor VMAD payload must encode")
}

fn power_armor_vmad_object_property(name: &str, form_id: u32) -> serde_json::Value {
    serde_json::json!({
        "propertyName": name,
        "Type": "Object",
        "Flags": VMAD_PROPERTY_FLAG_EDITED,
        "Value": {
            "Alias": -1,
            "FormID": {
                "reference": {
                    "plugin": FO4_MASTER_NAME,
                    "object_id": format!("{form_id:06X}"),
                },
            },
        },
    })
}

const FO76_XCRI_HEADER_SIZE: usize = 16;
const FO76_XCRI_MESH_ROW_SIZE: usize = 8;
const FO76_XCRI_REFERENCE_ROW_SIZE: usize = 16;
const FO4_XCRI_HEADER_SIZE: usize = 8;
const FO4_XCRI_MESH_ROW_SIZE: usize = 4;
const FO4_XCRI_REFERENCE_ROW_SIZE: usize = 8;

fn convert_cell_xcri_to_fo4(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
) -> Option<FieldValue> {
    match value {
        FieldValue::Bytes(bytes) => convert_cell_xcri_raw_to_fo4(bytes.as_slice())
            .map(|bytes| FieldValue::Bytes(smallvec::SmallVec::from_vec(bytes))),
        FieldValue::Struct(fields) => convert_cell_xcri_struct_to_fo4(fields, interner),
        _ => None,
    }
}

fn convert_cell_xcri_raw_to_fo4(bytes: &[u8]) -> Option<Vec<u8>> {
    if cell_xcri_fo4_raw_size(bytes)? == bytes.len() {
        return Some(bytes.to_vec());
    }
    if bytes.len() < FO76_XCRI_HEADER_SIZE {
        return None;
    }

    let mesh_count_u64 = u64::from_le_bytes(bytes[0..8].try_into().ok()?);
    let reference_count_u64 = u64::from_le_bytes(bytes[8..16].try_into().ok()?);
    let mesh_count = u32::try_from(mesh_count_u64).ok()?;
    let reference_count = u32::try_from(reference_count_u64).ok()?;
    let mesh_count_usize = mesh_count as usize;
    let reference_count_usize = reference_count as usize;
    let mesh_bytes_len = mesh_count_usize.checked_mul(FO76_XCRI_MESH_ROW_SIZE)?;
    let reference_bytes_len = reference_count_usize.checked_mul(FO76_XCRI_REFERENCE_ROW_SIZE)?;
    let reference_start = FO76_XCRI_HEADER_SIZE.checked_add(mesh_bytes_len)?;
    let expected_len = reference_start.checked_add(reference_bytes_len)?;
    if bytes.len() != expected_len {
        return None;
    }

    let mut out = Vec::with_capacity(
        FO4_XCRI_HEADER_SIZE
            + mesh_count_usize * FO4_XCRI_MESH_ROW_SIZE
            + reference_count_usize * FO4_XCRI_REFERENCE_ROW_SIZE,
    );
    out.extend_from_slice(&mesh_count.to_le_bytes());
    out.extend_from_slice(&reference_count.to_le_bytes());

    for index in 0..mesh_count_usize {
        let start = FO76_XCRI_HEADER_SIZE + index * FO76_XCRI_MESH_ROW_SIZE;
        out.extend_from_slice(bytes.get(start..start + 4)?);
    }

    for index in 0..reference_count_usize {
        let start = reference_start + index * FO76_XCRI_REFERENCE_ROW_SIZE;
        out.extend_from_slice(bytes.get(start..start + 4)?);
        out.extend_from_slice(bytes.get(start + 8..start + 12)?);
    }

    Some(out)
}

fn cell_xcri_fo4_raw_size(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < FO4_XCRI_HEADER_SIZE {
        return None;
    }
    let mesh_count = u32::from_le_bytes(bytes[0..4].try_into().ok()?) as usize;
    let reference_count = u32::from_le_bytes(bytes[4..8].try_into().ok()?) as usize;
    FO4_XCRI_HEADER_SIZE
        .checked_add(mesh_count.checked_mul(FO4_XCRI_MESH_ROW_SIZE)?)?
        .checked_add(reference_count.checked_mul(FO4_XCRI_REFERENCE_ROW_SIZE)?)
}

fn convert_cell_xcri_struct_to_fo4(
    fields: &[(crate::sym::Sym, FieldValue)],
    interner: &crate::sym::StringInterner,
) -> Option<FieldValue> {
    if named_value(fields, "meshes_count", interner).is_none()
        && named_value(fields, "references_count", interner).is_none()
        && named_value(fields, "meshes", interner).is_none()
        && named_value(fields, "references", interner).is_none()
    {
        return None;
    }

    let meshes = match named_value(fields, "meshes", interner) {
        Some(FieldValue::List(items)) => items
            .iter()
            .map(|item| project_cell_xcri_mesh(item, interner))
            .collect::<Option<Vec<_>>>()?,
        Some(_) => return None,
        None => Vec::new(),
    };
    let references = match named_value(fields, "references", interner) {
        Some(FieldValue::List(items)) => items
            .iter()
            .map(|item| project_cell_xcri_reference(item, interner))
            .collect::<Option<Vec<_>>>()?,
        Some(_) => return None,
        None => Vec::new(),
    };

    Some(FieldValue::Struct(vec![
        (
            interner.intern("meshes_count"),
            FieldValue::Uint(meshes.len() as u64),
        ),
        (
            interner.intern("references_count"),
            FieldValue::Uint(references.len() as u64),
        ),
        (interner.intern("meshes"), FieldValue::List(meshes)),
        (interner.intern("references"), FieldValue::List(references)),
    ]))
}

fn project_cell_xcri_mesh(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
) -> Option<FieldValue> {
    match value {
        FieldValue::Struct(fields) => {
            project_u32_value(named_value(fields, "combined_mesh", interner)?)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= FO76_XCRI_MESH_ROW_SIZE => {
            Some(bytes_value(bytes.get(0..4)?))
        }
        FieldValue::Bytes(bytes) if bytes.len() >= FO4_XCRI_MESH_ROW_SIZE => {
            Some(bytes_value(bytes.get(0..4)?))
        }
        FieldValue::Uint(_) | FieldValue::Int(_) => project_u32_value(value),
        _ => None,
    }
}

fn project_cell_xcri_reference(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
) -> Option<FieldValue> {
    match value {
        FieldValue::Struct(fields) => Some(FieldValue::Struct(vec![
            (
                interner.intern("reference"),
                project_formid_value(named_value(fields, "reference", interner)?)?,
            ),
            (
                interner.intern("combined_mesh"),
                project_u32_value(named_value(fields, "combined_mesh", interner)?)?,
            ),
        ])),
        FieldValue::Bytes(bytes) if bytes.len() >= FO76_XCRI_REFERENCE_ROW_SIZE => {
            Some(FieldValue::Struct(vec![
                (interner.intern("reference"), bytes_value(bytes.get(0..4)?)),
                (
                    interner.intern("combined_mesh"),
                    bytes_value(bytes.get(8..12)?),
                ),
            ]))
        }
        FieldValue::Bytes(bytes) if bytes.len() >= FO4_XCRI_REFERENCE_ROW_SIZE => {
            Some(FieldValue::Struct(vec![
                (interner.intern("reference"), bytes_value(bytes.get(0..4)?)),
                (
                    interner.intern("combined_mesh"),
                    bytes_value(bytes.get(4..8)?),
                ),
            ]))
        }
        _ => None,
    }
}

fn project_u32_value(value: &FieldValue) -> Option<FieldValue> {
    match value {
        FieldValue::Uint(value) if u32::try_from(*value).is_ok() => Some(FieldValue::Uint(*value)),
        FieldValue::Int(value) if u32::try_from(*value).is_ok() => Some(FieldValue::Int(*value)),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => Some(bytes_value(bytes.get(0..4)?)),
        _ => None,
    }
}

fn source_lvlo_reference(
    value: &FieldValue,
    source_plugin: crate::sym::Sym,
    interner: &crate::sym::StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::Uint(value) => u32::try_from(*value)
            .ok()
            .map(|raw| source_form_key_from_raw(raw, source_plugin)),
        FieldValue::Int(value) => u32::try_from(*value)
            .ok()
            .map(|raw| source_form_key_from_raw(raw, source_plugin)),
        FieldValue::Bytes(bytes) if bytes.len() >= 12 => {
            let raw = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
            Some(source_form_key_from_raw(raw, source_plugin))
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            Some(source_form_key_from_raw(raw, source_plugin))
        }
        FieldValue::Struct(fields) => {
            for name in [
                "value",
                "reference",
                "reference_reference",
                "base_data_item",
                "base_data_npc",
                "base_data_spell",
                "pack_in",
                "item",
                "npc",
                "spell",
            ] {
                if let Some(candidate) = named_value(fields, name, interner)
                    .and_then(|candidate| source_lvlo_reference(candidate, source_plugin, interner))
                {
                    return Some(candidate);
                }
            }
            fields.iter().find_map(|(_, candidate)| {
                source_lvlo_reference(candidate, source_plugin, interner)
            })
        }
        FieldValue::List(items) => items
            .iter()
            .find_map(|candidate| source_lvlo_reference(candidate, source_plugin, interner)),
        _ => None,
    }
}

fn source_form_key_from_raw(raw: u32, source_plugin: crate::sym::Sym) -> FormKey {
    FormKey {
        local: raw & 0x00FF_FFFF,
        plugin: source_plugin,
    }
}

fn npc_perk_entry_value(
    value: &FieldValue,
    source_plugin: crate::sym::Sym,
    perk_key: crate::sym::Sym,
    rank_key: crate::sym::Sym,
) -> Option<FieldValue> {
    let (raw, rank) = match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            (raw, bytes.get(4).copied().unwrap_or(0))
        }
        FieldValue::Uint(value) => (u32::try_from(*value).ok()?, 0),
        FieldValue::Int(value) => (u32::try_from(*value).ok()?, 0),
        _ => return None,
    };

    Some(FieldValue::Struct(vec![
        (
            perk_key,
            FieldValue::FormKey(source_form_key_from_raw(raw, source_plugin)),
        ),
        (rank_key, bytes_value(&[rank])),
    ]))
}

fn remap_known_lvlo_reference(
    interner: &crate::sym::StringInterner,
    reference: FormKey,
) -> FormKey {
    if reference.local == FO76_CAPS_FORM_ID
        && interner
            .resolve(reference.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO76_MASTER_NAME))
    {
        return FormKey {
            local: FO76_CAPS_FORM_ID,
            plugin: interner.intern(FO4_MASTER_NAME),
        };
    }
    reference
}

fn raw_lvlo_u16(value: &FieldValue, offset: usize) -> Option<u16> {
    let FieldValue::Bytes(bytes) = value else {
        return None;
    };
    if bytes.len() < 12 {
        return None;
    }
    let end = offset.checked_add(2)?;
    let slice = bytes.get(offset..end)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn npc_faction_value(
    interner: &crate::sym::StringInterner,
    value: &FieldValue,
    source_plugin: crate::sym::Sym,
) -> Option<FieldValue> {
    let FieldValue::Bytes(bytes) = value else {
        return None;
    };
    if bytes.len() != 5 {
        return None;
    }
    let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    Some(FieldValue::Struct(vec![
        (
            interner.intern("faction"),
            FieldValue::FormKey(source_form_key_from_raw(raw, source_plugin)),
        ),
        (interner.intern("rank"), bytes_value(&bytes[4..5])),
    ]))
}

fn npc_container_value(
    interner: &crate::sym::StringInterner,
    value: &FieldValue,
    source_plugin: crate::sym::Sym,
) -> Option<FieldValue> {
    let FieldValue::Bytes(bytes) = value else {
        return None;
    };
    if bytes.len() != 8 {
        return None;
    }
    let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    Some(FieldValue::Struct(vec![
        (
            interner.intern("item"),
            FieldValue::FormKey(source_form_key_from_raw(raw, source_plugin)),
        ),
        (interner.intern("count"), bytes_value(&bytes[4..8])),
    ]))
}

fn source_form_key_value(value: &FieldValue, source_plugin: crate::sym::Sym) -> Option<FieldValue> {
    match value {
        FieldValue::FormKey(_) => Some(value.clone()),
        FieldValue::Uint(value) => u32::try_from(*value)
            .ok()
            .map(|raw| FieldValue::FormKey(source_form_key_from_raw(raw, source_plugin))),
        FieldValue::Int(value) => u32::try_from(*value)
            .ok()
            .map(|raw| FieldValue::FormKey(source_form_key_from_raw(raw, source_plugin))),
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            Some(FieldValue::FormKey(source_form_key_from_raw(
                raw,
                source_plugin,
            )))
        }
        _ => None,
    }
}

fn push_scen_action_with_fo4_choices(
    source_plugin: crate::sym::Sym,
    row: Vec<FieldEntry>,
    retained: &mut smallvec::SmallVec<[FieldEntry; 8]>,
) {
    let mut stripped: Vec<FieldEntry> = Vec::with_capacity(row.len());
    let mut choices: Vec<FieldValue> = Vec::new();

    for entry in row {
        match &entry.sig.0 {
            b"ESCS" => {
                if choices.len() < SCEN_PLAYER_RESPONSE_SIGS.len()
                    && let Some(value) = source_form_key_value(&entry.value, source_plugin)
                {
                    choices.push(value);
                }
            }
            b"ESCE" => {}
            _ => stripped.push(entry),
        }
    }

    if choices.is_empty() {
        retained.extend(stripped);
        return;
    }

    let insert_at = stripped
        .iter()
        .position(|entry| entry.sig.0 == *b"DTGT")
        .unwrap_or(stripped.len());

    for entry in stripped.drain(..insert_at) {
        retained.push(entry);
    }
    for (index, value) in choices.iter().cloned().enumerate() {
        retained.push(FieldEntry {
            sig: SubrecordSig(SCEN_PLAYER_RESPONSE_SIGS[index]),
            value,
        });
    }
    for (index, value) in choices.into_iter().enumerate() {
        retained.push(FieldEntry {
            sig: SubrecordSig(SCEN_NPC_RESPONSE_SIGS[index]),
            value,
        });
    }
    retained.extend(stripped);
}

fn following_u16_value(
    fields: &[FieldEntry],
    start: usize,
    wanted_sig: &[u8; 4],
    default_value: u16,
) -> u16 {
    for entry in fields.iter().skip(start) {
        if entry.sig.0 == *b"LVLO" {
            break;
        }
        if entry.sig.0 == *wanted_sig {
            return field_value_to_u16(&entry.value).unwrap_or(default_value);
        }
    }
    default_value
}

fn field_value_to_u16(value: &FieldValue) -> Option<u16> {
    match value {
        FieldValue::Uint(value) => u16::try_from(*value).ok(),
        FieldValue::Int(value) => u16::try_from(*value).ok(),
        FieldValue::Float(value) if value.is_finite() => {
            let rounded = value.round();
            (0.0..=u16::MAX as f32)
                .contains(&rounded)
                .then_some(rounded as u16)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            Some(u16::from_le_bytes([bytes[0], bytes[1]]))
        }
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(_, candidate)| field_value_to_u16(candidate)),
        _ => None,
    }
}

fn fo4_lvlo_value(
    interner: &crate::sym::StringInterner,
    record_sig: &[u8; 4],
    level: u16,
    reference: FormKey,
    count: u16,
) -> FieldValue {
    let reference_field = match record_sig {
        b"LVLN" => "npc",
        b"LVSP" => "spell",
        _ => "item",
    };
    FieldValue::Struct(vec![
        (interner.intern("level"), bytes_value(&level.to_le_bytes())),
        (interner.intern("unknown_u8_1"), bytes_value(&[0])),
        (interner.intern("unknown_u8_2"), bytes_value(&[0])),
        (
            interner.intern(reference_field),
            FieldValue::FormKey(reference),
        ),
        (interner.intern("count"), bytes_value(&count.to_le_bytes())),
        (interner.intern("chance_none"), bytes_value(&[0])),
        (interner.intern("unknown_u8_6"), bytes_value(&[0])),
    ])
}

fn strip_record_obts_properties(
    interner: &crate::sym::StringInterner,
    record: &mut Record,
    target: ObjectModPropertyTarget,
) {
    for entry in &mut record.fields {
        if entry.sig.0 == *b"OBTS" {
            strip_object_template_property_rows(interner, &mut entry.value, target);
        }
    }
}

/// Remove every entry from a decoded FormKey-array subrecord value whose
/// object-id (lower 24 bits) is in `drop_object_ids`. Handles the decoded
/// `List<FormKey>` form (the FormKey path also checks `source_plugin` so a FO4
/// master ref with a colliding local is never dropped) and the raw `Bytes`
/// formid-array fallback (matched on the lower 24 bits only).
fn filter_formkey_array_value(
    value: &mut FieldValue,
    source_plugin: crate::sym::Sym,
    drop_object_ids: &[u32],
) {
    match value {
        FieldValue::List(items) => {
            items.retain(|item| !formkey_array_item_matches(item, source_plugin, drop_object_ids))
        }
        FieldValue::Bytes(bytes) => {
            let mut out = Vec::with_capacity(bytes.len());
            for chunk in bytes.chunks(4) {
                if let [a, b, c, d] = chunk {
                    let raw = u32::from_le_bytes([*a, *b, *c, *d]);
                    if drop_object_ids.contains(&(raw & 0x00FF_FFFF)) {
                        continue;
                    }
                }
                out.extend_from_slice(chunk);
            }
            *bytes = smallvec::SmallVec::from_vec(out);
        }
        _ => {}
    }
}

fn formkey_array_item_matches(
    value: &FieldValue,
    source_plugin: crate::sym::Sym,
    drop_object_ids: &[u32],
) -> bool {
    match value {
        FieldValue::FormKey(fk) => {
            fk.plugin == source_plugin && drop_object_ids.contains(&(fk.local & 0x00FF_FFFF))
        }
        FieldValue::Uint(raw) => drop_object_ids.contains(&((*raw as u32) & 0x00FF_FFFF)),
        FieldValue::Int(raw) if *raw >= 0 => {
            drop_object_ids.contains(&((*raw as u32) & 0x00FF_FFFF))
        }
        FieldValue::Bytes(bytes) if bytes.len() == 4 => drop_object_ids.contains(
            &(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) & 0x00FF_FFFF),
        ),
        _ => false,
    }
}

fn formkey_array_value_is_empty(value: &FieldValue) -> bool {
    match value {
        FieldValue::List(items) => items.is_empty(),
        FieldValue::Bytes(bytes) => bytes.is_empty(),
        _ => false,
    }
}

fn strip_omod_data_properties(interner: &crate::sym::StringInterner, record: &mut Record) {
    for entry in &mut record.fields {
        if entry.sig.0 != *b"DATA" {
            continue;
        }
        let target = omod_property_target(&entry.value, interner);
        strip_omod_data_property_rows(interner, &mut entry.value, target);
    }
}

fn omod_data_form_type(value: &FieldValue, interner: &crate::sym::StringInterner) -> Option<u32> {
    match value {
        FieldValue::Struct(fields) => {
            named_value_canonical(fields, "formtype", interner).and_then(field_value_to_u32)
        }
        FieldValue::Bytes(bytes) => read_u32_le_at(bytes, OMOD_DATA_FORM_TYPE_OFFSET),
        _ => None,
    }
}

fn omod_property_target(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
) -> ObjectModPropertyTarget {
    let form_type = omod_data_form_type(value, interner);
    let Some(form_type) = form_type else {
        return ObjectModPropertyTarget::Object;
    };
    match &form_type.to_le_bytes() {
        b"WEAP" => ObjectModPropertyTarget::Weapon,
        b"ARMO" | b"ARMA" => ObjectModPropertyTarget::Armor,
        b"NPC_" => ObjectModPropertyTarget::Actor,
        _ => ObjectModPropertyTarget::Object,
    }
}

impl Fo76Fo4Hook {
    fn drop_mstt_omod_data(interner: &crate::sym::StringInterner, record: &mut Record) {
        if record.sig.0 != *b"OMOD" {
            return;
        }
        record.fields.retain(|entry| {
            entry.sig.0 != *b"DATA"
                || omod_data_form_type(&entry.value, interner)
                    .is_none_or(|form_type| form_type != FO76_MSTT_FORM_TYPE)
        });
    }
}

fn omod_has_material_swap_data(interner: &crate::sym::StringInterner, record: &Record) -> bool {
    if record.fields.iter().any(|entry| entry.sig.0 == *b"MODS") {
        return true;
    }

    record
        .fields
        .iter()
        .filter(|entry| entry.sig.0 == *b"DATA")
        .any(|entry| {
            let target = omod_property_target(&entry.value, interner);
            material_swap_property_id(target).is_some_and(|property_id| {
                omod_data_has_property_id(&entry.value, interner, property_id)
            })
        })
}

fn material_swap_property_id(target: ObjectModPropertyTarget) -> Option<u16> {
    match target {
        ObjectModPropertyTarget::Weapon => Some(89),
        ObjectModPropertyTarget::Armor => Some(13),
        ObjectModPropertyTarget::Actor => Some(5),
        ObjectModPropertyTarget::Object => None,
    }
}

fn omod_data_has_property_id(
    value: &FieldValue,
    interner: &crate::sym::StringInterner,
    property_id: u16,
) -> bool {
    match value {
        FieldValue::Struct(fields) => {
            let Some(FieldValue::List(properties)) =
                named_value_canonical(fields, "properties", interner)
            else {
                return false;
            };
            properties.iter().any(|property| {
                property_id_from_row(property, interner).is_some_and(|id| id == property_id)
            })
        }
        FieldValue::Bytes(bytes) => raw_omod_data_has_property_id(bytes, property_id),
        _ => false,
    }
}

fn raw_omod_data_has_property_id(bytes: &[u8], property_id: u16) -> bool {
    if bytes.len() < OMOD_DATA_HEADER_LEN + OMOD_DATA_ATTACH_PARENT_SLOT_COUNT_LEN {
        return false;
    }
    let Some(include_count) = read_u32_le_at(bytes, 0).map(|count| count as usize) else {
        return false;
    };
    let Some(property_count) =
        read_u32_le_at(bytes, OBJECT_TEMPLATE_PROPERTY_COUNT_OFFSET).map(|count| count as usize)
    else {
        return false;
    };
    let Some(includes_len) = include_count.checked_mul(OMOD_DATA_INCLUDE_ROW_LEN) else {
        return false;
    };
    let Some(attach_parent_slot_count) =
        read_u32_le_at(bytes, OMOD_DATA_HEADER_LEN).map(|count| count as usize)
    else {
        return false;
    };
    let Some(attach_parent_slots_len) = attach_parent_slot_count.checked_mul(4) else {
        return false;
    };
    let slots_start = OMOD_DATA_HEADER_LEN + OMOD_DATA_ATTACH_PARENT_SLOT_COUNT_LEN;
    let Some(item_start) = slots_start.checked_add(attach_parent_slots_len) else {
        return false;
    };
    let Some(includes_start) = item_start.checked_add(OMOD_DATA_ITEM_ROW_LEN) else {
        return false;
    };
    let Some(properties_start) = includes_start.checked_add(includes_len) else {
        return false;
    };

    (0..property_count).any(|index| {
        let row_start = properties_start + index * OBJECT_MOD_PROPERTY_ROW_LEN;
        let Some(property_bytes) = bytes.get(
            row_start + OBJECT_MOD_PROPERTY_ID_OFFSET
                ..row_start + OBJECT_MOD_PROPERTY_ID_OFFSET + 2,
        ) else {
            return false;
        };
        u16::from_le_bytes([property_bytes[0], property_bytes[1]]) == property_id
    })
}

fn strip_object_template_property_rows(
    interner: &crate::sym::StringInterner,
    value: &mut FieldValue,
    target: ObjectModPropertyTarget,
) -> u32 {
    match value {
        FieldValue::Struct(_) => strip_struct_property_rows(interner, value, target),
        FieldValue::Bytes(bytes) => strip_raw_object_template_property_rows(bytes, target),
        _ => 0,
    }
}

fn strip_omod_data_property_rows(
    interner: &crate::sym::StringInterner,
    value: &mut FieldValue,
    target: ObjectModPropertyTarget,
) -> u32 {
    match value {
        FieldValue::Struct(_) => strip_struct_property_rows(interner, value, target),
        FieldValue::Bytes(bytes) => strip_raw_omod_data_property_rows(bytes, target),
        _ => 0,
    }
}

fn set_omod_data_property_function_type(
    interner: &crate::sym::StringInterner,
    value: &mut FieldValue,
    property_id: u16,
    function_type: u8,
) -> u32 {
    match value {
        FieldValue::Struct(_) => {
            set_struct_property_function_type(interner, value, property_id, function_type)
        }
        FieldValue::Bytes(bytes) => {
            set_raw_omod_data_property_function_type(bytes, property_id, function_type)
        }
        _ => 0,
    }
}

fn set_struct_property_function_type(
    interner: &crate::sym::StringInterner,
    value: &mut FieldValue,
    property_id: u16,
    function_type: u8,
) -> u32 {
    let FieldValue::Struct(fields) = value else {
        return 0;
    };
    let Some(properties_index) = field_index_canonical(fields, "properties", interner) else {
        return 0;
    };
    let FieldValue::List(properties) = &mut fields[properties_index].1 else {
        return 0;
    };

    let mut changed = 0_u32;
    for property in properties {
        if property_id_from_row(property, interner).is_some_and(|id| id == property_id)
            && set_property_row_function_type(interner, property, function_type)
        {
            changed += 1;
        }
    }
    changed
}

fn set_property_row_function_type(
    interner: &crate::sym::StringInterner,
    value: &mut FieldValue,
    function_type: u8,
) -> bool {
    match value {
        FieldValue::Struct(fields) => {
            if let Some(index) = field_index_canonical(fields, "functiontype", interner) {
                if field_value_to_u16(&fields[index].1) == Some(function_type as u16) {
                    return false;
                }
                set_u32_count(&mut fields[index].1, function_type as u32);
                return true;
            }
            fields.push((
                interner.intern("FunctionType"),
                FieldValue::Uint(function_type as u64),
            ));
            true
        }
        FieldValue::Bytes(bytes) if bytes.len() > OBJECT_MOD_PROPERTY_FUNCTION_TYPE_OFFSET => {
            if bytes[OBJECT_MOD_PROPERTY_FUNCTION_TYPE_OFFSET] == function_type {
                return false;
            }
            bytes[OBJECT_MOD_PROPERTY_FUNCTION_TYPE_OFFSET] = function_type;
            true
        }
        _ => false,
    }
}

fn set_raw_omod_data_property_function_type(
    bytes: &mut smallvec::SmallVec<[u8; 32]>,
    property_id: u16,
    function_type: u8,
) -> u32 {
    if bytes.len() < OMOD_DATA_HEADER_LEN + OMOD_DATA_ATTACH_PARENT_SLOT_COUNT_LEN {
        return 0;
    }
    let Some(include_count) = read_u32_le_at(bytes, 0).map(|count| count as usize) else {
        return 0;
    };
    let Some(property_count) =
        read_u32_le_at(bytes, OBJECT_TEMPLATE_PROPERTY_COUNT_OFFSET).map(|count| count as usize)
    else {
        return 0;
    };
    let Some(includes_len) = include_count.checked_mul(OMOD_DATA_INCLUDE_ROW_LEN) else {
        return 0;
    };
    let Some(attach_parent_slot_count) =
        read_u32_le_at(bytes, OMOD_DATA_HEADER_LEN).map(|count| count as usize)
    else {
        return 0;
    };
    let Some(attach_parent_slots_len) = attach_parent_slot_count.checked_mul(4) else {
        return 0;
    };
    let slots_start = OMOD_DATA_HEADER_LEN + OMOD_DATA_ATTACH_PARENT_SLOT_COUNT_LEN;
    let Some(item_start) = slots_start.checked_add(attach_parent_slots_len) else {
        return 0;
    };
    let Some(includes_start) = item_start.checked_add(OMOD_DATA_ITEM_ROW_LEN) else {
        return 0;
    };
    let Some(properties_start) = includes_start.checked_add(includes_len) else {
        return 0;
    };
    let Some(properties_len) = property_count.checked_mul(OBJECT_MOD_PROPERTY_ROW_LEN) else {
        return 0;
    };
    let Some(properties_end) = properties_start.checked_add(properties_len) else {
        return 0;
    };
    if properties_end > bytes.len() {
        return 0;
    }

    let mut changed = 0_u32;
    for index in 0..property_count {
        let row_start = properties_start + index * OBJECT_MOD_PROPERTY_ROW_LEN;
        let Some(property_bytes) = bytes.get(
            row_start + OBJECT_MOD_PROPERTY_ID_OFFSET
                ..row_start + OBJECT_MOD_PROPERTY_ID_OFFSET + 2,
        ) else {
            continue;
        };
        if u16::from_le_bytes([property_bytes[0], property_bytes[1]]) != property_id {
            continue;
        }
        let function_type_offset = row_start + OBJECT_MOD_PROPERTY_FUNCTION_TYPE_OFFSET;
        if bytes[function_type_offset] != function_type {
            bytes[function_type_offset] = function_type;
            changed += 1;
        }
    }
    changed
}

fn strip_struct_property_rows(
    interner: &crate::sym::StringInterner,
    value: &mut FieldValue,
    target: ObjectModPropertyTarget,
) -> u32 {
    let FieldValue::Struct(fields) = value else {
        return 0;
    };
    let Some(properties_index) = field_index_canonical(fields, "properties", interner) else {
        return 0;
    };

    let Some((removed, kept_count)) =
        strip_properties_value(interner, &mut fields[properties_index].1, target)
    else {
        return 0;
    };
    if removed == 0 {
        return 0;
    }

    if let Some(count_index) = field_index_canonical(fields, "propertycount", interner) {
        set_u32_count(&mut fields[count_index].1, kept_count as u32);
    }
    removed
}

fn strip_raw_object_template_property_rows(
    bytes: &mut smallvec::SmallVec<[u8; 32]>,
    target: ObjectModPropertyTarget,
) -> u32 {
    if bytes.len() < OBJECT_TEMPLATE_FIXED_HEADER_LEN + 2 {
        return 0;
    }
    let Some(include_count) = read_u32_le_at(bytes, 0).map(|count| count as usize) else {
        return 0;
    };
    let Some(property_count) =
        read_u32_le_at(bytes, OBJECT_TEMPLATE_PROPERTY_COUNT_OFFSET).map(|count| count as usize)
    else {
        return 0;
    };
    let keyword_count = bytes[OBJECT_TEMPLATE_KEYWORD_COUNT_OFFSET] as usize;
    let Some(properties_start) = keyword_count
        .checked_mul(4)
        .and_then(|len| OBJECT_TEMPLATE_FIXED_HEADER_LEN.checked_add(len))
        .and_then(|offset| offset.checked_add(2))
        .and_then(|offset| {
            include_count
                .checked_mul(OBJECT_TEMPLATE_INCLUDE_ROW_LEN)
                .and_then(|len| offset.checked_add(len))
        })
    else {
        return 0;
    };

    strip_raw_object_mod_property_rows(
        bytes,
        OBJECT_TEMPLATE_PROPERTY_COUNT_OFFSET,
        properties_start,
        property_count,
        target,
    )
}

fn strip_raw_omod_data_property_rows(
    bytes: &mut smallvec::SmallVec<[u8; 32]>,
    target: ObjectModPropertyTarget,
) -> u32 {
    if bytes.len() < OMOD_DATA_HEADER_LEN + OMOD_DATA_ATTACH_PARENT_SLOT_COUNT_LEN {
        return 0;
    }
    let Some(include_count) = read_u32_le_at(bytes, 0).map(|count| count as usize) else {
        return 0;
    };
    let Some(property_count) =
        read_u32_le_at(bytes, OBJECT_TEMPLATE_PROPERTY_COUNT_OFFSET).map(|count| count as usize)
    else {
        return 0;
    };
    let Some(includes_len) = include_count.checked_mul(OMOD_DATA_INCLUDE_ROW_LEN) else {
        return 0;
    };
    let Some(properties_len) = property_count.checked_mul(OBJECT_MOD_PROPERTY_ROW_LEN) else {
        return 0;
    };
    let Some(attach_parent_slot_count) =
        read_u32_le_at(bytes, OMOD_DATA_HEADER_LEN).map(|count| count as usize)
    else {
        return 0;
    };
    let Some(attach_parent_slots_len) = attach_parent_slot_count.checked_mul(4) else {
        return 0;
    };
    let slots_start = OMOD_DATA_HEADER_LEN + OMOD_DATA_ATTACH_PARENT_SLOT_COUNT_LEN;
    let Some(item_start) = slots_start.checked_add(attach_parent_slots_len) else {
        return 0;
    };
    let Some(includes_start) = item_start.checked_add(OMOD_DATA_ITEM_ROW_LEN) else {
        return 0;
    };
    let Some(properties_start) = includes_start.checked_add(includes_len) else {
        return 0;
    };
    let Some(properties_end) = properties_start.checked_add(properties_len) else {
        return 0;
    };
    if properties_end > bytes.len() {
        return 0;
    }

    strip_raw_object_mod_property_rows(
        bytes,
        OBJECT_TEMPLATE_PROPERTY_COUNT_OFFSET,
        properties_start,
        property_count,
        target,
    )
}

fn strip_raw_object_mod_property_rows(
    bytes: &mut smallvec::SmallVec<[u8; 32]>,
    property_count_offset: usize,
    properties_start: usize,
    property_count: usize,
    target: ObjectModPropertyTarget,
) -> u32 {
    let Some(properties_len) = property_count.checked_mul(OBJECT_MOD_PROPERTY_ROW_LEN) else {
        return 0;
    };
    let Some(properties_end) = properties_start.checked_add(properties_len) else {
        return 0;
    };
    if properties_end > bytes.len() {
        return 0;
    }

    let mut kept = Vec::with_capacity(properties_len);
    let mut kept_count = 0_usize;
    for index in 0..property_count {
        let row_start = properties_start + index * OBJECT_MOD_PROPERTY_ROW_LEN;
        let row_end = row_start + OBJECT_MOD_PROPERTY_ROW_LEN;
        let row = &bytes[row_start..row_end];
        let property_id = u16::from_le_bytes([
            row[OBJECT_MOD_PROPERTY_ID_OFFSET],
            row[OBJECT_MOD_PROPERTY_ID_OFFSET + 1],
        ]);
        if valid_object_mod_property(target, property_id) {
            kept.extend_from_slice(row);
            kept_count += 1;
        }
    }

    if kept_count == property_count {
        return 0;
    }
    let suffix = bytes[properties_end..].to_vec();
    bytes.truncate(properties_start);
    bytes.extend_from_slice(&kept);
    bytes.extend_from_slice(&suffix);
    set_u32_le_at(bytes, property_count_offset, kept_count as u32);
    (property_count - kept_count) as u32
}

fn strip_properties_value(
    interner: &crate::sym::StringInterner,
    value: &mut FieldValue,
    target: ObjectModPropertyTarget,
) -> Option<(u32, usize)> {
    let FieldValue::List(properties) = value else {
        return None;
    };
    let before = properties.len();
    properties.retain(|property| {
        property_id_from_row(property, interner)
            .is_some_and(|property_id| valid_object_mod_property(target, property_id))
    });
    Some(((before - properties.len()) as u32, properties.len()))
}

fn property_id_from_row(value: &FieldValue, interner: &crate::sym::StringInterner) -> Option<u16> {
    match value {
        FieldValue::Struct(fields) => {
            named_value_canonical(fields, "property", interner).and_then(field_value_to_u16)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 10 => Some(u16::from_le_bytes([
            bytes[OBJECT_MOD_PROPERTY_ID_OFFSET],
            bytes[OBJECT_MOD_PROPERTY_ID_OFFSET + 1],
        ])),
        _ => None,
    }
}

fn valid_object_mod_property(target: ObjectModPropertyTarget, property_id: u16) -> bool {
    match target {
        ObjectModPropertyTarget::Weapon => property_id <= 94,
        ObjectModPropertyTarget::Armor => property_id <= 13,
        ObjectModPropertyTarget::Actor => property_id <= 5,
        ObjectModPropertyTarget::Object => false,
    }
}

fn read_u32_le_at(bytes: &[u8], offset: usize) -> Option<u32> {
    bytes
        .get(offset..offset + 4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
}

fn set_u32_le_at(bytes: &mut [u8], offset: usize, value: u32) {
    if let Some(chunk) = bytes.get_mut(offset..offset + 4) {
        chunk.copy_from_slice(&value.to_le_bytes());
    }
}

fn sync_llct_count(record: &mut Record, count: usize) {
    let count = count.min(u8::MAX as usize) as u64;
    let llct_sig = SubrecordSig(*b"LLCT");
    if let Some(entry) = record.fields.iter_mut().find(|entry| entry.sig == llct_sig) {
        entry.value = FieldValue::Uint(count);
    } else {
        record.fields.insert(
            0,
            FieldEntry {
                sig: llct_sig,
                value: FieldValue::Uint(count),
            },
        );
    }
}

fn project_formid_value(value: &FieldValue) -> Option<FieldValue> {
    match value {
        FieldValue::FormKey(_) => Some(value.clone()),
        FieldValue::Uint(value) if u32::try_from(*value).is_ok() => Some(FieldValue::Uint(*value)),
        FieldValue::Int(value) if u32::try_from(*value).is_ok() => Some(FieldValue::Int(*value)),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => Some(bytes_value(bytes.get(0..4)?)),
        _ => None,
    }
}

fn field_value_to_u32(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        FieldValue::Float(value) if value.is_finite() => {
            let rounded = value.round();
            (0.0..=u32::MAX as f32)
                .contains(&rounded)
                .then_some(rounded as u32)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        }
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(_, candidate)| field_value_to_u32(candidate)),
        _ => None,
    }
}

fn field_value_to_i64(value: &FieldValue) -> Option<i64> {
    match value {
        FieldValue::Uint(value) => i64::try_from(*value).ok(),
        FieldValue::Int(value) => Some(*value),
        FieldValue::Float(value) if value.is_finite() => Some(value.round() as i64),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64)
        }
        _ => None,
    }
}

fn destructible_header_health(
    interner: &crate::sym::StringInterner,
    value: &FieldValue,
) -> Option<i64> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64)
        }
        FieldValue::Struct(fields) => named_value_canonical(fields, "health", interner)
            .and_then(field_value_to_i64)
            .or_else(|| {
                named_value_canonical(fields, "header", interner)
                    .and_then(|header| destructible_header_health(interner, header))
            }),
        _ => field_value_to_i64(value),
    }
}

fn named_value<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &crate::sym::StringInterner,
) -> Option<&'a FieldValue> {
    let key = interner.intern(name);
    fields
        .iter()
        .find_map(|(field_name, value)| (*field_name == key).then_some(value))
}

fn named_value_canonical<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &crate::sym::StringInterner,
) -> Option<&'a FieldValue> {
    field_index_canonical(fields, name, interner).map(|index| &fields[index].1)
}

fn field_index_canonical(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &crate::sym::StringInterner,
) -> Option<usize> {
    let wanted = canonical_field_name(name);
    fields.iter().position(|(field_name, _)| {
        interner
            .resolve(*field_name)
            .is_some_and(|field_name| canonical_field_name(field_name) == wanted)
    })
}

fn canonical_field_name(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn bytes_value(bytes: &[u8]) -> FieldValue {
    FieldValue::Bytes(smallvec::SmallVec::from_slice(bytes))
}

fn set_u32_zero(value: &mut FieldValue) {
    match value {
        FieldValue::Uint(n) => *n = 0,
        FieldValue::Int(n) => *n = 0,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            bytes[..4].copy_from_slice(&0_u32.to_le_bytes());
        }
        FieldValue::Bytes(bytes) => {
            bytes.clear();
            bytes.extend_from_slice(&0_u32.to_le_bytes());
        }
        _ => *value = FieldValue::Uint(0),
    }
}

fn set_u32_count(value: &mut FieldValue, count: u32) {
    match value {
        FieldValue::Uint(n) => *n = count as u64,
        FieldValue::Int(n) => *n = count as i64,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            bytes[..4].copy_from_slice(&count.to_le_bytes());
        }
        FieldValue::Bytes(bytes) => {
            bytes.clear();
            bytes.extend_from_slice(&count.to_le_bytes());
        }
        FieldValue::Struct(fields) => {
            if let Some((_, first_value)) = fields.first_mut() {
                set_u32_count(first_value, count);
            }
        }
        _ => *value = FieldValue::Uint(count as u64),
    }
}

fn set_u32_bits(value: &mut FieldValue, mask: u32) {
    match value {
        FieldValue::Uint(n) => *n |= mask as u64,
        FieldValue::Int(n) => *n = ((*n as u32) | mask) as i64,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let mut raw = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
            raw |= mask;
            bytes[0..4].copy_from_slice(&raw.to_le_bytes());
        }
        FieldValue::Bytes(bytes) => {
            bytes.clear();
            bytes.extend_from_slice(&mask.to_le_bytes());
        }
        FieldValue::Struct(fields) => {
            if let Some((_, first_value)) = fields.first_mut() {
                set_u32_bits(first_value, mask);
            }
        }
        _ => *value = FieldValue::Uint(mask as u64),
    }
}

fn mark_qust_alias_fnam_optional(value: &mut FieldValue) {
    set_u32_bits(value, QUST_ALIAS_OPTIONAL_FLAG);
}

fn clear_u32_bits(value: &mut FieldValue, mask: u32) {
    match value {
        FieldValue::Uint(n) => *n &= !(mask as u64),
        FieldValue::Int(n) => *n = ((*n as u32) & !mask) as i64,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let mut raw = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
            raw &= !mask;
            bytes[0..4].copy_from_slice(&raw.to_le_bytes());
        }
        FieldValue::Struct(fields) => {
            if let Some((_, first_value)) = fields.first_mut() {
                clear_u32_bits(first_value, mask);
            }
        }
        _ => {}
    }
}

fn furniture_record_has_model(record: &Record) -> bool {
    record.fields.iter().any(|entry| {
        entry.sig.0 == *b"MNAM" && u32_field_has_bits(&entry.value, FURNITURE_HAS_MODEL_BIT)
    })
}

fn u32_field_has_bits(value: &FieldValue, mask: u32) -> bool {
    let raw = match value {
        FieldValue::Uint(n) => *n as u32,
        FieldValue::Int(n) => *n as u32,
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            u32::from_le_bytes(bytes[0..4].try_into().unwrap())
        }
        FieldValue::Struct(fields) => match fields.first() {
            Some((_, first_value)) => return u32_field_has_bits(first_value, mask),
            None => return false,
        },
        _ => return false,
    };
    raw & mask != 0
}

fn target_furniture_marker_count(record: &Record) -> usize {
    let mut count = 0usize;
    for entry in &record.fields {
        let entry_count = match &entry.sig.0 {
            b"SNAM" => marker_parameters_row_count(&entry.value),
            b"FNPR" | b"ENAM" | b"NAM0" => marker_entry_count(&entry.value),
            _ => 0,
        };
        count = count.max(entry_count);
    }
    count
}

fn marker_entry_count(value: &FieldValue) -> usize {
    match value {
        FieldValue::None => 0,
        FieldValue::List(items) => items.len(),
        _ => 1,
    }
}

fn marker_parameters_row_count(value: &FieldValue) -> usize {
    match value {
        FieldValue::None => 0,
        FieldValue::List(items) => items.len(),
        FieldValue::Bytes(bytes) => bytes.len() / FURNITURE_MARKER_PARAMETERS_ROW_LEN,
        _ => 1,
    }
}

fn truncate_raw_subrecord(record: &mut Record, sig: &[u8; 4], max_len: usize) {
    for entry in &mut record.fields {
        if entry.sig.0 != *sig {
            continue;
        }
        if let FieldValue::Bytes(bytes) = &mut entry.value {
            bytes.truncate(max_len);
        }
    }
}

fn project_raw_array_rows(
    record: &mut Record,
    sig: &[u8; 4],
    source_row_len: usize,
    target_row_len: usize,
) {
    if source_row_len <= target_row_len || target_row_len == 0 {
        return;
    }

    for entry in &mut record.fields {
        if entry.sig.0 != *sig {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        if bytes.is_empty() || bytes.len() % source_row_len != 0 {
            continue;
        }

        let mut projected = smallvec::SmallVec::new();
        for row in bytes.chunks_exact(source_row_len) {
            projected.extend_from_slice(&row[..target_row_len]);
        }
        *bytes = projected;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{FormKeyMapper, MapperOptions};
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::RecordFlags;
    use crate::record::{FieldEntry, FieldValue, Record};
    use crate::schema::AuthoringSchema;
    use crate::sym::StringInterner;
    use crate::translator::{Game, TranslateResult, Translator};
    use smallvec::SmallVec;

    fn make_ctx(interner: &StringInterner) -> PairCtx<'_> {
        PairCtx { interner }
    }

    fn make_record(sig: &str, interner: &StringInterner) -> Record {
        let fk = FormKey::parse("000800@SeventySix.esm", interner).unwrap();
        Record::new(SigCode::from_str(sig).unwrap(), fk)
    }

    fn push_field(record: &mut Record, sig: &str, value: FieldValue) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        });
    }

    #[test]
    fn pre_translate_upper_body_skin_drops_fo76_attachment_slots() {
        let interner = StringInterner::new();
        let mut record = make_record("ARMA", &interner);
        let attachment_slots = (41..=45).fold(0_u64, |mask, slot| mask | (1 << (slot - 30)));
        push_field(
            &mut record,
            "BOD2",
            FieldValue::Uint((1 << (33 - 30)) | attachment_slots | (1 << (60 - 30))),
        );
        push_field(
            &mut record,
            "XFLG",
            FieldValue::Uint(FO76_ARMA_XFLG_HAS_UPPER_BODY_SKIN),
        );

        let hook = Fo76Fo4Hook;
        hook.pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let mask = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"BOD2")
            .and_then(|entry| match entry.value {
                FieldValue::Uint(mask) => Some(mask),
                _ => None,
            })
            .expect("ARMA BOD2 mask");
        assert_eq!(mask, (1 << (33 - 30)) | (1 << (60 - 30)));
    }

    #[test]
    fn pre_translate_chinese_stealth_arma_keeps_pipboy_visible() {
        let interner = StringInterner::new();
        let mut record = make_record("ARMA", &interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("AA_ArmorChineseStealth")),
        );
        push_field(
            &mut record,
            "BOD2",
            FieldValue::Uint((1 << (33 - 30)) | (1 << (60 - 30))),
        );

        let hook = Fo76Fo4Hook;
        hook.pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let mask = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"BOD2")
            .and_then(|entry| match entry.value {
                FieldValue::Uint(mask) => Some(mask),
                _ => None,
            })
            .expect("ARMA BOD2 mask");
        assert_eq!(mask, 1 << (33 - 30));
    }

    fn raw_ctda(function_id: u16) -> FieldValue {
        raw_ctda_with_parameter_1(function_id, 0)
    }

    fn raw_ctda_with_parameter_1(function_id: u16, parameter_1: u32) -> FieldValue {
        let mut bytes = vec![0_u8; 32];
        bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        bytes[12..16].copy_from_slice(&parameter_1.to_le_bytes());
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    fn raw_ctda_with_run_on(function_id: u16, parameter_1: u32, run_on: u32) -> FieldValue {
        let mut bytes = vec![0_u8; 32];
        bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        bytes[12..16].copy_from_slice(&parameter_1.to_le_bytes());
        bytes[20..24].copy_from_slice(&run_on.to_le_bytes());
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    #[test]
    fn pre_translate_converts_nif_backed_empty_scol_to_stat() {
        let interner = StringInterner::new();
        let mut record = make_record("SCOL", &interner);
        let original_form_key = record.form_key;
        let eid = interner.intern("ToxicCreeperSC01_Copy02");
        record.eid = Some(eid);
        push_field(&mut record, "EDID", FieldValue::String(eid));
        push_field(
            &mut record,
            "OBND",
            FieldValue::Bytes(SmallVec::from_slice(&[0; 12])),
        );
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("SCOL\\SeventySix.esm\\CM007D2AB2.NIF")),
        );
        push_field(
            &mut record,
            "MODT",
            FieldValue::Bytes(SmallVec::from_slice(&[1, 2, 3, 4])),
        );
        push_field(
            &mut record,
            "ONAM",
            FieldValue::Bytes(SmallVec::from_slice(&[0; 8])),
        );
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(SmallVec::from_slice(&[0; 28])),
        );
        push_field(&mut record, "DEFL", FieldValue::Uint(0xA4E1));

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(record.sig, SigCode::from_str("STAT").unwrap());
        assert_eq!(record.form_key, original_form_key);
        assert_eq!(record.eid, Some(eid));
        assert_eq!(
            record
                .fields
                .iter()
                .map(|entry| entry.sig.as_str())
                .collect::<Vec<_>>(),
            vec!["EDID", "OBND", "MODL", "MODT"]
        );
    }

    #[test]
    fn pre_translate_keeps_scol_with_a_usable_part() {
        let interner = StringInterner::new();
        let mut record = make_record("SCOL", &interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("SCOL\\SeventySix.esm\\CM00001234.NIF")),
        );
        push_field(
            &mut record,
            "ONAM",
            FieldValue::Bytes(SmallVec::from_slice(&[0x12, 0x58, 0x03, 0x00, 0, 0, 0, 0])),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        assert_eq!(record.sig, SigCode::from_str("SCOL").unwrap());
        assert!(record.fields.iter().any(|entry| entry.sig.0 == *b"ONAM"));
    }

    #[test]
    fn pre_translate_keeps_empty_scol_without_a_non_empty_model() {
        let interner = StringInterner::new();
        for model in [None, Some(" \t\0 ")] {
            let mut record = make_record("SCOL", &interner);
            if let Some(model) = model {
                push_field(
                    &mut record,
                    "MODL",
                    FieldValue::String(interner.intern(model)),
                );
            }
            push_field(&mut record, "ONAM", FieldValue::None);

            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            assert_eq!(record.sig, SigCode::from_str("SCOL").unwrap());
        }
    }

    #[test]
    fn pre_translate_stat_signature_drives_mapper_and_flst_rewrite() {
        let interner = StringInterner::new();
        let translator = Translator::new(Game::Fo76, Game::Fo4).unwrap();
        let mut source = make_record("SCOL", &interner);
        let source_form_key = source.form_key;
        let eid = interner.intern("EmptyCombinedStatic");
        source.eid = Some(eid);
        push_field(&mut source, "EDID", FieldValue::String(eid));
        push_field(
            &mut source,
            "MODL",
            FieldValue::String(interner.intern("SCOL\\SeventySix.esm\\CM00000800.NIF")),
        );

        translator
            .pre_translate(&mut make_ctx(&interner), &mut source)
            .unwrap();
        let mut translated = match translator.translate(&source, &interner) {
            TranslateResult::Translated(record) => record,
            other => panic!("expected translated record, got {other:?}"),
        };
        assert_eq!(translated.sig, SigCode::from_str("STAT").unwrap());
        assert_eq!(translated.form_key, source_form_key);

        let stat_target = FormKey::parse("001234@Fallout4.esm", &interner).unwrap();
        let scol_target = FormKey::parse("005678@Fallout4.esm", &interner).unwrap();
        let mut mapper = FormKeyMapper::new(
            [
                (eid, scol_target, SigCode::from_str("SCOL").unwrap()),
                (eid, stat_target, SigCode::from_str("STAT").unwrap()),
            ],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".to_string(),
                use_base_game_assets: true,
                ..Default::default()
            },
            &interner,
        );
        let target = mapper.allocate_or_resolve(source_form_key, Some(eid), translated.sig);
        translated.form_key = target;
        assert_eq!(target, stat_target, "mapper must select the STAT EID entry");

        let mut flst = make_record("FLST", &interner);
        push_field(&mut flst, "LNAM", FieldValue::FormKey(source_form_key));
        mapper.rewrite_record(&mut flst).unwrap();
        assert!(matches!(
            flst.fields.first().map(|entry| &entry.value),
            Some(FieldValue::FormKey(form_key)) if *form_key == stat_target
        ));
    }

    fn raw_bytes(bytes: &[u8]) -> FieldValue {
        FieldValue::Bytes(SmallVec::from_slice(bytes))
    }

    #[test]
    fn post_translate_namespaces_raw_radio_receiver_frequency_once() {
        let interner = StringInterner::new();
        let mut record = make_record("ACTI", &interner);
        let mut receiver = vec![0_u8; 14];
        receiver[4..8].copy_from_slice(&98.2_f32.to_le_bytes());
        push_field(&mut record, "RADR", raw_bytes(&receiver));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();
        let once = record.fields.clone();
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Bytes(receiver) = &record.fields[0].value else {
            panic!("expected raw RADR");
        };
        assert_eq!(
            f32::from_le_bytes(receiver[4..8].try_into().unwrap()),
            98.2 + FO76_RADIO_FREQUENCY_NAMESPACE_OFFSET
        );
        assert_eq!(record.fields, once);
    }

    #[test]
    fn post_translate_namespaces_structured_radio_receiver_frequency() {
        let interner = StringInterner::new();
        let mut record = make_record("ACTI", &interner);
        push_field(
            &mut record,
            "RADR",
            FieldValue::Struct(vec![
                (interner.intern("SoundModel"), FieldValue::Uint(0x0B5183)),
                (interner.intern("Frequency"), FieldValue::Float(80.5)),
            ]),
        );

        Fo76Fo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("expected structured RADR");
        };
        assert_eq!(
            fields[1].1,
            FieldValue::Float(80.5 + FO76_RADIO_FREQUENCY_NAMESPACE_OFFSET)
        );
    }

    #[test]
    fn pre_translate_strips_fo76_info_editor_id() {
        let interner = StringInterner::new();
        let mut record = make_record("INFO", &interner);
        push_field(&mut record, "ENAM", FieldValue::None);
        push_field(&mut record, "EDID", raw_bytes(b"FO76OnlyInfoEditorId\0"));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["ENAM"]);
    }

    #[test]
    fn pre_translate_drops_only_unanchored_term_condition_groups() {
        let interner = StringInterner::new();
        let mut record = make_record("TERM", &interner);
        push_field(&mut record, "BSIZ", raw_bytes(&1_u32.to_le_bytes()));
        push_field(&mut record, "BTXT", FieldValue::None);
        push_field(&mut record, "CTDA", raw_ctda(1));
        push_field(&mut record, "CIS1", FieldValue::None);
        push_field(&mut record, "ISIZ", raw_bytes(&1_u32.to_le_bytes()));
        push_field(&mut record, "CTDA", raw_ctda(2));
        push_field(&mut record, "CIS1", FieldValue::None);
        push_field(&mut record, "CIS2", FieldValue::None);
        push_field(&mut record, "ITXT", FieldValue::None);
        push_field(&mut record, "ANAM", FieldValue::None);
        push_field(&mut record, "ITID", FieldValue::None);
        push_field(&mut record, "CTDA", raw_ctda(3));
        push_field(&mut record, "CIS2", FieldValue::None);
        push_field(&mut record, "UNAM", FieldValue::None);
        push_field(&mut record, "CTDA", raw_ctda(4));
        push_field(&mut record, "CIS1", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(
            sigs,
            vec![
                "BSIZ", "BTXT", "CTDA", "CIS1", "ISIZ", "ITXT", "ANAM", "ITID", "CTDA", "CIS2",
                "UNAM",
            ]
        );
    }

    #[test]
    fn pre_translate_maps_refr_marker_types_to_two_byte_fo4_layout() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);

        for (source_type, target_type) in
            [(2_u16, 3_u8), (7, 8), (54, 58), (64, 6), (65, 4), (66, 8)]
        {
            let mut record = make_record("REFR", &interner);
            push_field(
                &mut record,
                "TNAM",
                raw_bytes(&u32::from(source_type).to_le_bytes()),
            );

            hook.pre_translate(&mut ctx, &mut record).unwrap();

            match &record.fields[0].value {
                FieldValue::Bytes(bytes) => assert_eq!(bytes.as_slice(), &[target_type, 0]),
                value => panic!("expected TNAM bytes, got {value:?}"),
            }
        }
    }

    #[test]
    fn pre_translate_does_not_rewrite_non_refr_tnam() {
        let interner = StringInterner::new();
        let mut record = make_record("TERM", &interner);
        push_field(&mut record, "TNAM", raw_bytes(&64_u32.to_le_bytes()));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        match &record.fields[0].value {
            FieldValue::Bytes(bytes) => assert_eq!(bytes.as_slice(), &64_u32.to_le_bytes()),
            value => panic!("expected TNAM bytes, got {value:?}"),
        }
    }

    #[test]
    fn zero_health_cont_destructible_strip_spans_fo76_interstitials() {
        let interner = StringInterner::new();
        let mut record = make_record("CONT", &interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "DEST", raw_bytes(&0_i32.to_le_bytes()));
        push_field(&mut record, "HGLB", raw_bytes(&[1, 0, 0, 0]));
        push_field(&mut record, "DSTD", raw_bytes(&[0; 28]));
        push_field(&mut record, "DMDL", raw_bytes(b"destroyed.nif\0"));
        push_field(&mut record, "DMDT", raw_bytes(&[0; 20]));
        push_field(&mut record, "ENLT", raw_bytes(&[0; 4]));
        push_field(&mut record, "ENLS", raw_bytes(&[0; 4]));
        push_field(&mut record, "AUUV", raw_bytes(&[0; 32]));
        push_field(&mut record, "DSTF", FieldValue::None);
        push_field(&mut record, "DATA", raw_bytes(&[1]));

        Fo76Fo4Hook::strip_zero_health_cont_destructibles(&interner, &mut record);

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["EDID", "DATA"]);
    }

    #[test]
    fn nonzero_health_cont_destructible_is_preserved() {
        let interner = StringInterner::new();
        let mut record = make_record("CONT", &interner);
        push_field(&mut record, "DEST", raw_bytes(&50_i32.to_le_bytes()));
        push_field(&mut record, "HGLB", raw_bytes(&[1, 0, 0, 0]));
        push_field(&mut record, "DSTD", raw_bytes(&[0; 28]));
        push_field(&mut record, "DMDL", raw_bytes(b"destroyed.nif\0"));
        push_field(&mut record, "DMDT", raw_bytes(&[0; 20]));
        push_field(&mut record, "ENLT", raw_bytes(&[0; 4]));
        push_field(&mut record, "ENLS", raw_bytes(&[0; 4]));
        push_field(&mut record, "AUUV", raw_bytes(&[0; 32]));
        push_field(&mut record, "DSTF", FieldValue::None);
        push_field(&mut record, "DATA", raw_bytes(&[1]));

        Fo76Fo4Hook::strip_zero_health_cont_destructibles(&interner, &mut record);

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(
            sigs,
            vec![
                "DEST", "HGLB", "DSTD", "DMDL", "DMDT", "ENLT", "ENLS", "AUUV", "DSTF", "DATA",
            ]
        );
    }

    fn read_vmad_string(bytes: &[u8], offset: &mut usize) -> String {
        let length = u16::from_le_bytes(bytes[*offset..*offset + 2].try_into().unwrap()) as usize;
        *offset += 2;
        let value = std::str::from_utf8(&bytes[*offset..*offset + length])
            .unwrap()
            .to_string();
        *offset += length;
        value
    }

    fn read_power_armor_vmad(bytes: &[u8]) -> (String, Vec<(String, u32)>) {
        let mut offset = 0;
        assert_eq!(
            u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()),
            FO4_VMAD_VERSION
        );
        offset += 2;
        assert_eq!(
            u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()),
            FO4_VMAD_OBJECT_FORMAT
        );
        offset += 2;
        assert_eq!(
            u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()),
            1
        );
        offset += 2;

        let script_name = read_vmad_string(bytes, &mut offset);
        assert_eq!(bytes[offset], 0);
        offset += 1;
        let property_count =
            u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()) as usize;
        offset += 2;

        let mut properties = Vec::with_capacity(property_count);
        for _ in 0..property_count {
            let name = read_vmad_string(bytes, &mut offset);
            assert_eq!(bytes[offset], 1);
            offset += 1;
            assert_eq!(bytes[offset], VMAD_PROPERTY_FLAG_EDITED);
            offset += 1;
            assert_eq!(
                u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()),
                0
            );
            offset += 2;
            assert_eq!(
                i16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()),
                -1
            );
            offset += 2;
            let form_id = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            offset += 4;
            properties.push((name, form_id));
        }
        assert_eq!(offset, bytes.len());
        (script_name, properties)
    }

    #[test]
    fn pre_translate_strips_scorched_statue_activation_conditions() {
        let interner = StringInterner::new();
        let mut record = make_record("ACTI", &interner);
        record.eid = Some(interner.intern(SCORCHED_STATUE_ACTI_EID));
        push_field(&mut record, "FULL", FieldValue::None);
        push_field(&mut record, "CNDC", raw_bytes(&0_u32.to_le_bytes()));
        push_field(&mut record, "CITC", raw_bytes(&3_u32.to_le_bytes()));
        push_field(&mut record, "CTDA", raw_ctda(203));
        push_field(&mut record, "CTDA", raw_ctda(77));
        push_field(&mut record, "CTDA", raw_ctda(828));
        push_field(&mut record, "CNDC", raw_bytes(&1_u32.to_le_bytes()));
        push_field(&mut record, "CITC", raw_bytes(&1_u32.to_le_bytes()));
        push_field(&mut record, "CTDA", raw_ctda(203));
        push_field(&mut record, "FNAM", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["FULL", "FNAM"]);
    }

    #[test]
    fn pre_translate_keeps_unrelated_acti_activation_conditions() {
        let interner = StringInterner::new();
        let mut record = make_record("ACTI", &interner);
        record.eid = Some(interner.intern("OtherSearchActivator"));
        push_field(&mut record, "CITC", raw_bytes(&1_u32.to_le_bytes()));
        push_field(&mut record, "CTDA", raw_ctda(203));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["CITC", "CTDA"]);
    }

    #[test]
    fn pre_translate_renames_only_furniture_marker_parameters() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);

        let mut furniture = make_record("FURN", &interner);
        push_field(
            &mut furniture,
            "ZNAM",
            raw_bytes(&[0_u8; FURNITURE_MARKER_PARAMETERS_ROW_LEN]),
        );
        hook.pre_translate(&mut ctx, &mut furniture).unwrap();
        assert_eq!(furniture.fields[0].sig.as_str(), "SNAM");

        let mut terminal = make_record("TERM", &interner);
        push_field(&mut terminal, "SNAM", raw_bytes(&1_u32.to_le_bytes()));
        push_field(
            &mut terminal,
            "ZNAM",
            raw_bytes(&[0_u8; FURNITURE_MARKER_PARAMETERS_ROW_LEN]),
        );
        hook.pre_translate(&mut ctx, &mut terminal).unwrap();

        let sigs: Vec<&str> = terminal
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["SNAM", "ZNAM"]);
    }

    #[test]
    fn pre_translate_remaps_structured_dial_misc_category_to_fo4() {
        let interner = StringInterner::new();
        let mut record = make_record("DIAL", &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (interner.intern("topic_flags"), FieldValue::Uint(0)),
                (
                    interner.intern("category"),
                    FieldValue::Uint(u64::from(FO76_DIAL_CATEGORY_MISCELLANEOUS)),
                ),
                (interner.intern("subtype"), FieldValue::Uint(118)),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let data = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DATA")
            .expect("DATA remains");
        let FieldValue::Struct(fields) = &data.value else {
            panic!("expected structured DIAL DATA");
        };
        let category = fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("category"))
            .map(|(_, value)| value)
            .expect("category field remains");
        assert_eq!(
            category,
            &FieldValue::Uint(u64::from(FO4_DIAL_CATEGORY_MISCELLANEOUS))
        );
    }

    #[test]
    fn pre_translate_remaps_raw_dial_detection_category_to_fo4() {
        let interner = StringInterner::new();
        let mut record = make_record("DIAL", &interner);
        push_field(
            &mut record,
            "DATA",
            raw_bytes(&[0, FO76_DIAL_CATEGORY_DETECTION, 88, 0]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let data = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DATA")
            .expect("DATA remains");
        assert_eq!(
            data.value,
            raw_bytes(&[0, FO4_DIAL_CATEGORY_DETECTION, 88, 0])
        );
    }

    #[test]
    fn pre_translate_maps_scen_escs_choices_to_fo4_player_dialogue_slots() {
        let interner = StringInterner::new();
        let mut record = make_record("SCEN", &interner);
        push_field(&mut record, "ANAM", FieldValue::Uint(3));
        push_field(&mut record, "INAM", FieldValue::Uint(56));
        push_field(&mut record, "DTGT", FieldValue::Int(0));
        push_field(&mut record, "ESCE", raw_bytes(&0x56A146_u32.to_le_bytes()));
        push_field(&mut record, "ESCS", raw_bytes(&0x56A145_u32.to_le_bytes()));
        push_field(&mut record, "ESCE", raw_bytes(&0x56A144_u32.to_le_bytes()));
        push_field(&mut record, "ESCS", raw_bytes(&0x56A143_u32.to_le_bytes()));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(
            sigs,
            vec!["ANAM", "INAM", "PTOP", "NTOP", "NPOT", "NNGT", "DTGT"]
        );

        let source_plugin = interner.intern(FO76_MASTER_NAME);
        assert_eq!(
            record.fields[2].value,
            FieldValue::FormKey(FormKey {
                local: 0x56A145,
                plugin: source_plugin
            })
        );
        assert_eq!(record.fields[2].value, record.fields[4].value);
        assert_eq!(
            record.fields[3].value,
            FieldValue::FormKey(FormKey {
                local: 0x56A143,
                plugin: source_plugin
            })
        );
        assert_eq!(record.fields[3].value, record.fields[5].value);
    }

    #[test]
    fn pre_translate_resets_scen_choice_slots_for_each_action() {
        let interner = StringInterner::new();
        let mut record = make_record("SCEN", &interner);
        push_field(&mut record, "ANAM", FieldValue::Uint(3));
        push_field(&mut record, "ESCS", raw_bytes(&0x56A145_u32.to_le_bytes()));
        push_field(&mut record, "ANAM", FieldValue::Uint(3));
        push_field(&mut record, "ESCS", raw_bytes(&0x58F9DB_u32.to_le_bytes()));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["ANAM", "PTOP", "NPOT", "ANAM", "PTOP", "NPOT"]);
    }

    #[test]
    fn pre_translate_drops_wrld_runtime_tables() {
        let interner = StringInterner::new();
        let mut record = make_record("WRLD", &interner);
        for sig in ["EDID", "RNAM", "MHDT", "OFST", "CLSZ", "NAM0"] {
            push_field(&mut record, sig, raw_bytes(&[0, 1, 2, 3]));
        }

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<_> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["EDID", "NAM0"]);
    }

    #[test]
    fn drop_incompatible_condition_reconciles_citc() {
        // CITC=2 with one compatible condition (fn 74) and one FO76-only
        // condition (fn 875 > FO4 max 817). Dropping the incompatible CTDA
        // must also decrement CITC to match, or FO4's audio update
        // null-derefs on the phantom condition.
        let interner = StringInterner::new();
        let mut record = make_record("MUST", &interner);
        push_field(&mut record, "CNAM", raw_bytes(&0u32.to_le_bytes()));
        push_field(&mut record, "CITC", raw_bytes(&2u32.to_le_bytes()));
        push_field(&mut record, "CTDA", raw_ctda(74));
        push_field(&mut record, "CTDA", raw_ctda(875));

        Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut record);

        let ctda_count = record
            .fields
            .iter()
            .filter(|f| f.sig.as_str() == "CTDA")
            .count();
        assert_eq!(ctda_count, 1, "fn 875 condition dropped");
        let citc = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "CITC")
            .expect("CITC remains");
        assert_eq!(
            citc.value,
            raw_bytes(&1u32.to_le_bytes()),
            "CITC reconciled to surviving CTDA count"
        );
    }

    #[test]
    fn drop_incompatible_conditions_reconciles_preexisting_citc_mismatch() {
        let interner = StringInterner::new();
        let mut record = make_record("MUST", &interner);
        push_field(&mut record, "CITC", raw_bytes(&2u32.to_le_bytes()));
        push_field(&mut record, "CTDA", raw_ctda(74));

        Fo76Fo4Hook::drop_fo4_incompatible_conditions(&interner, &mut record);

        let citc = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "CITC")
            .expect("CITC remains");
        assert_eq!(
            citc.value,
            raw_bytes(&1u32.to_le_bytes()),
            "preexisting stale CITC reconciled even when this hook drops nothing"
        );
    }

    #[test]
    fn pre_translate_converts_raw_npc_prkr_to_typed_formkey() {
        let interner = StringInterner::new();
        let mut record = make_record("NPC_", &interner);
        push_field(
            &mut record,
            "PRKR",
            raw_bytes(&[0xF5, 0x64, 0x84, 0x00, 0x02]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let prkr = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "PRKR")
            .expect("PRKR remains");
        let FieldValue::Struct(fields) = &prkr.value else {
            panic!("PRKR should be structured");
        };
        let FieldValue::FormKey(perk) =
            named_value(fields, "Perk", &interner).expect("perk reference")
        else {
            panic!("PRKR perk should be a FormKey");
        };
        assert_eq!(perk.local, 0x8464F5);
        assert_eq!(interner.resolve(perk.plugin), Some("SeventySix.esm"));

        let expected_rank = raw_bytes(&[0x02]);
        assert_eq!(
            named_value(fields, "Rank", &interner).expect("perk rank"),
            &expected_rank
        );
    }

    #[test]
    fn pre_translate_converts_note_snam_scene_to_typed_formkey() {
        let interner = StringInterner::new();
        let mut record = make_record("NOTE", &interner);
        push_field(&mut record, "SNAM", FieldValue::Uint(0x0053_4F51));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let snam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "SNAM")
            .expect("SNAM remains");
        let FieldValue::FormKey(scene) = &snam.value else {
            panic!("SNAM should be a FormKey");
        };
        assert_eq!(scene.local, 0x534F51);
        assert_eq!(interner.resolve(scene.plugin), Some("SeventySix.esm"));
    }

    #[test]
    fn normalized_note_snam_participates_in_formkey_mapper() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NOTE", &interner);
        push_field(&mut record, "SNAM", FieldValue::Uint(0x0053_4F51));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let source_fk = FormKey {
            local: 0x534F51,
            plugin: interner.intern("SeventySix.esm"),
        };
        let target_fk = FormKey {
            local: 0x534F51,
            plugin: interner.intern("Output.esm"),
        };
        let mut mapper = FormKeyMapper::new(
            Vec::new(),
            MapperOptions {
                output_plugin_name: "Output.esm".to_string(),
                ..Default::default()
            },
            &mut interner,
        );
        mapper.add_mapping(source_fk, target_fk);
        mapper.rewrite_record(&mut record).unwrap();

        let snam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "SNAM")
            .expect("SNAM remains");
        assert_eq!(snam.value, FieldValue::FormKey(target_fk));
    }

    #[test]
    fn normalized_npc_snam_participates_in_formkey_mapper() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", &interner);
        push_field(
            &mut record,
            "SNAM",
            raw_bytes(&[0x04, 0x83, 0x05, 0x00, 0x00]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let source_fk = FormKey {
            local: 0x058304,
            plugin: interner.intern("SeventySix.esm"),
        };
        let target_fk = FormKey {
            local: 0x058304,
            plugin: interner.intern("Fallout4.esm"),
        };
        let mut mapper = FormKeyMapper::new(
            Vec::new(),
            MapperOptions {
                output_plugin_name: "SeventySix.esm".to_string(),
                ..Default::default()
            },
            &mut interner,
        );
        mapper.add_mapping(source_fk, target_fk);
        mapper.rewrite_record(&mut record).unwrap();

        let snam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "SNAM")
            .expect("SNAM remains");
        let FieldValue::Struct(fields) = &snam.value else {
            panic!("SNAM should be structured");
        };
        assert_eq!(
            named_value(fields, "faction", &interner).expect("faction"),
            &FieldValue::FormKey(target_fk)
        );
    }

    #[test]
    fn normalized_npc_cnto_participates_in_formkey_mapper() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", &interner);
        push_field(
            &mut record,
            "CNTO",
            raw_bytes(&[0x3B, 0x33, 0x11, 0x00, 1, 0, 0, 0]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let source_fk = FormKey {
            local: 0x11333B,
            plugin: interner.intern("SeventySix.esm"),
        };
        let target_fk = FormKey {
            local: 0x11333B,
            plugin: interner.intern("Fallout4.esm"),
        };
        let mut mapper = FormKeyMapper::new(
            Vec::new(),
            MapperOptions {
                output_plugin_name: "SeventySix.esm".to_string(),
                ..Default::default()
            },
            &mut interner,
        );
        mapper.add_mapping(source_fk, target_fk);
        mapper.rewrite_record(&mut record).unwrap();

        let cnto = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "CNTO")
            .expect("CNTO remains");
        let FieldValue::Struct(fields) = &cnto.value else {
            panic!("CNTO should be structured");
        };
        assert_eq!(
            named_value(fields, "item", &interner).expect("item"),
            &FieldValue::FormKey(target_fk)
        );
    }

    #[test]
    fn normalized_cont_cnto_participates_in_formkey_mapper() {
        let mut interner = StringInterner::new();
        let mut record = make_record("CONT", &interner);
        push_field(
            &mut record,
            "CNTO",
            raw_bytes(&[0xB5, 0x73, 0x06, 0x00, 1, 0, 0, 0]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let source_fk = FormKey {
            local: 0x0673B5,
            plugin: interner.intern("SeventySix.esm"),
        };
        let target_fk = FormKey {
            local: 0x0673B5,
            plugin: interner.intern("Fallout4.esm"),
        };
        let mut mapper = FormKeyMapper::new(
            Vec::new(),
            MapperOptions {
                output_plugin_name: "SeventySix.esm".to_string(),
                ..Default::default()
            },
            &mut interner,
        );
        mapper.add_mapping(source_fk, target_fk);
        mapper.rewrite_record(&mut record).unwrap();

        let cnto = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "CNTO")
            .expect("CNTO remains");
        let FieldValue::Struct(fields) = &cnto.value else {
            panic!("CNTO should be structured");
        };
        assert_eq!(
            named_value(fields, "item", &interner).expect("item"),
            &FieldValue::FormKey(target_fk)
        );
    }

    #[test]
    fn normalized_npc_prkr_participates_in_formkey_mapper() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", &interner);
        push_field(
            &mut record,
            "PRKR",
            raw_bytes(&[0xF5, 0x64, 0x84, 0x00, 0x00]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let source_fk = FormKey {
            local: 0x8464F5,
            plugin: interner.intern("SeventySix.esm"),
        };
        let target_fk = FormKey {
            local: 0x8464F5,
            plugin: interner.intern("Output.esm"),
        };
        let mut mapper = FormKeyMapper::new(
            Vec::new(),
            MapperOptions {
                output_plugin_name: "Output.esm".to_string(),
                ..Default::default()
            },
            &mut interner,
        );
        mapper.add_mapping(source_fk, target_fk);
        mapper.rewrite_record(&mut record).unwrap();

        let prkr = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "PRKR")
            .expect("PRKR remains");
        let FieldValue::Struct(fields) = &prkr.value else {
            panic!("PRKR should be structured");
        };
        assert_eq!(
            named_value(fields, "Perk", &interner).expect("perk reference"),
            &FieldValue::FormKey(target_fk)
        );
    }

    // -------------------------------------------------------------------------
    // Behavior 1: global field drop
    // -------------------------------------------------------------------------

    #[test]
    fn pre_translate_drops_magf_subrecord() {
        let mut interner = StringInterner::new();
        let mut record = make_record("WEAP", &mut interner);
        push_field(&mut record, "MAGF", FieldValue::None);
        push_field(&mut record, "EDID", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"MAGF"), "MAGF should be dropped");
        assert!(sigs.contains(&"EDID"), "EDID should be preserved");
    }

    #[test]
    fn pre_translate_drops_codv_subrecord() {
        let mut interner = StringInterner::new();
        let mut record = make_record("ARMO", &mut interner);
        push_field(&mut record, "CODV", FieldValue::None);
        push_field(
            &mut record,
            "FULL",
            FieldValue::String(interner.intern("Armor")),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"CODV"));
        assert!(sigs.contains(&"FULL"));
    }

    #[test]
    fn pre_translate_drops_version_control_sigs() {
        let mut interner = StringInterner::new();
        let mut record = make_record("WEAP", &mut interner);
        // Drop VCTX (VersionControl) and FVER (FormVersion)
        push_field(&mut record, "VCTX", FieldValue::None);
        push_field(&mut record, "FVER", FieldValue::None);
        push_field(&mut record, "EDID", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"VCTX"));
        assert!(!sigs.contains(&"FVER"));
        assert!(sigs.contains(&"EDID"));
    }

    #[test]
    fn pre_translate_drops_all_global_sigs() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", &mut interner);
        for sig in &[
            "VCTX", "FVER", "FL76", "FLWR", "MIID", "MAGF", "CODV", "OPDS",
        ] {
            push_field(&mut record, sig, FieldValue::None);
        }
        push_field(&mut record, "EDID", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(record.fields.len(), 1);
        assert_eq!(record.fields[0].sig.as_str(), "EDID");
    }

    #[test]
    fn pre_translate_converts_fo76_lvli_split_rows_to_fo4_lvlo() {
        let mut interner = StringInterner::new();
        let mut record = make_record("LVLI", &mut interner);
        let variant_sym = interner.intern("variant");
        let value_sym = interner.intern("value");
        let reference_variant = interner.intern("reference");
        push_field(&mut record, "LLCT", FieldValue::Uint(2));
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Struct(vec![
                (variant_sym, FieldValue::String(reference_variant)),
                (value_sym, FieldValue::Uint(0x08E3A8)),
            ]),
        );
        push_field(&mut record, "LVIV", FieldValue::Float(2.0));
        push_field(&mut record, "LVLV", FieldValue::Float(3.0));
        push_field(&mut record, "LVLO", FieldValue::Uint(0x02C59E));
        push_field(&mut record, "LVIV", FieldValue::Float(1.0));
        push_field(&mut record, "LVLV", FieldValue::Float(1.0));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let count = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "LLCT")
            .map(|entry| &entry.value);
        assert_eq!(count, Some(&FieldValue::Uint(2)));
        let lvlo_entries: Vec<_> = record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "LVLO")
            .collect();
        assert_eq!(lvlo_entries.len(), 2);

        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let record =
            match crate::target_normalize::TargetRecordNormalizer::target_only_with_interner(
                &schema, &interner,
            )
            .normalize(record)
            {
                crate::target_normalize::TargetRecordNormalization::Keep(record) => record,
                crate::target_normalize::TargetRecordNormalization::DropUnsupportedRecord => {
                    panic!("LVLI is supported by FO4 schema")
                }
            };
        let lvlo_entries: Vec<_> = record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "LVLO")
            .collect();
        assert_eq!(lvlo_entries.len(), 2);
        let first = crate::target_write::encode_field_pub(
            lvlo_entries[0],
            schema.record_def("LVLI"),
            &interner,
        )
        .expect("first converted LVLO encodes");
        assert_eq!(first, vec![3, 0, 0, 0, 0xA8, 0xE3, 0x08, 0, 2, 0, 0, 0]);
        let second = crate::target_write::encode_field_pub(
            lvlo_entries[1],
            schema.record_def("LVLI"),
            &interner,
        )
        .expect("second converted LVLO encodes");
        assert_eq!(second, vec![1, 0, 0, 0, 0x9E, 0xC5, 0x02, 0, 1, 0, 0, 0]);
    }

    /// CTDA with explicit operator (high 3 bits of the type byte) + comparison value.
    fn raw_ctda_full(function_id: u16, operator: u8, comparison_value: f32) -> FieldValue {
        let mut bytes = vec![0_u8; 32];
        bytes[0] = operator << 5;
        bytes[4..8].copy_from_slice(&comparison_value.to_le_bytes());
        bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    #[test]
    fn condition_gates_dropped_world_state_classifies_nuke_and_event_globals() {
        // Nuke-zone check (func 849): dropped regardless of operator/value.
        assert!(Fo76Fo4Hook::condition_gates_dropped_world_state(
            &raw_ctda_full(FO76_NUKE_ZONE_CONDITION_FUNCTION_ID, 0, 0.0,)
        ));
        // GetGlobalValue == 1 (event ON): dropped.
        assert!(Fo76Fo4Hook::condition_gates_dropped_world_state(
            &raw_ctda_full(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID, 0, 1.0,)
        ));
        // GetGlobalValue != 0 (event ON): dropped.
        assert!(Fo76Fo4Hook::condition_gates_dropped_world_state(
            &raw_ctda_full(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID, 1, 0.0,)
        ));
        // GetGlobalValue >= 1 (event ON): dropped.
        assert!(Fo76Fo4Hook::condition_gates_dropped_world_state(
            &raw_ctda_full(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID, 3, 1.0,)
        ));
        // GetGlobalValue == 0 (event OFF / FO4 default): kept.
        assert!(!Fo76Fo4Hook::condition_gates_dropped_world_state(
            &raw_ctda_full(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID, 0, 0.0,)
        ));
        // Unrelated condition function: kept.
        assert!(!Fo76Fo4Hook::condition_gates_dropped_world_state(
            &raw_ctda_full(56, 0, 1.0)
        ));
    }

    #[test]
    fn pre_translate_drops_nuke_and_event_gated_leveled_entries() {
        let mut interner = StringInterner::new();
        let mut record = make_record("LVLI", &mut interner);
        push_field(&mut record, "LLCT", FieldValue::Uint(3));
        // Nuke-zone-gated entry (radiation suit) -> dropped.
        push_field(&mut record, "LVLO", FieldValue::Uint(0x58AFD6));
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_full(FO76_NUKE_ZONE_CONDITION_FUNCTION_ID, 0, 1.0),
        );
        // Festive entry gated on GetGlobalValue(event) == 1.0 -> dropped.
        push_field(&mut record, "LVLO", FieldValue::Uint(0x5A0019));
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_full(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID, 0, 1.0),
        );
        // Normal ungated entry -> kept.
        push_field(&mut record, "LVLO", FieldValue::Uint(0x58AFD5));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let lvlo: Vec<_> = record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "LVLO")
            .collect();
        assert_eq!(
            lvlo.len(),
            1,
            "nuke + event-gated entries dropped, normal kept"
        );
        let count = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "LLCT")
            .map(|entry| &entry.value);
        assert_eq!(count, Some(&FieldValue::Uint(1)));
    }

    #[test]
    fn pre_translate_converts_fo76_lvln_reference_to_fo4_npc_lvlo() {
        let mut interner = StringInterner::new();
        let mut record = make_record("LVLN", &mut interner);
        let variant_sym = interner.intern("variant");
        let value_sym = interner.intern("value");
        let reference_variant = interner.intern("reference");
        push_field(&mut record, "LLCT", FieldValue::Uint(1));
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Struct(vec![
                (variant_sym, FieldValue::String(reference_variant)),
                (value_sym, FieldValue::Uint(0x868BB8)),
            ]),
        );
        push_field(&mut record, "LVIV", FieldValue::Float(1.0));
        push_field(&mut record, "LVLV", FieldValue::Float(1.0));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let record =
            match crate::target_normalize::TargetRecordNormalizer::target_only_with_interner(
                &schema, &interner,
            )
            .normalize(record)
            {
                crate::target_normalize::TargetRecordNormalization::Keep(record) => record,
                crate::target_normalize::TargetRecordNormalization::DropUnsupportedRecord => {
                    panic!("LVLN is supported by FO4 schema")
                }
            };
        let lvlo_entry = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "LVLO")
            .expect("converted LVLO");
        let encoded =
            crate::target_write::encode_field_pub(lvlo_entry, schema.record_def("LVLN"), &interner)
                .expect("converted LVLO encodes");
        assert_eq!(encoded, vec![1, 0, 0, 0, 0xB8, 0x8B, 0x86, 0, 1, 0, 0, 0]);
    }

    #[test]
    fn pre_translate_converts_raw_fo76_lvlo_bytes_to_source_plugin_formkey() {
        let mut interner = StringInterner::new();
        let mut record = make_record("LVLI", &mut interner);
        let mut raw_lvlo = vec![1, 0, 0, 0];
        raw_lvlo.extend_from_slice(&0x0083_9C65_u32.to_le_bytes());
        raw_lvlo.extend_from_slice(&[1, 0, 0, 0]);
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Bytes(SmallVec::from_vec(raw_lvlo)),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let lvlo_entry = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "LVLO")
            .expect("converted LVLO");
        let FieldValue::Struct(fields) = &lvlo_entry.value else {
            panic!("LVLO should be converted into an FO4 struct");
        };
        let item = named_value(fields, "item", &interner).expect("item reference");
        let FieldValue::FormKey(fk) = item else {
            panic!("LVLO item should be a typed FormKey, got {item:?}");
        };
        assert_eq!(fk.local, 0x0083_9C65);
        assert_eq!(interner.resolve(fk.plugin), Some("SeventySix.esm"));
    }

    #[test]
    fn pre_translate_converts_four_byte_fo76_lvlo_reference_without_using_it_as_level() {
        let mut interner = StringInterner::new();
        let mut record = make_record("LVLI", &mut interner);
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Bytes(SmallVec::from_vec(0x0083_9C65_u32.to_le_bytes().to_vec())),
        );
        push_field(&mut record, "LVIV", FieldValue::Float(7.0));
        push_field(&mut record, "LVLV", FieldValue::Float(2.0));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let record =
            match crate::target_normalize::TargetRecordNormalizer::target_only_with_interner(
                &schema, &interner,
            )
            .normalize(record)
            {
                crate::target_normalize::TargetRecordNormalization::Keep(record) => record,
                crate::target_normalize::TargetRecordNormalization::DropUnsupportedRecord => {
                    panic!("LVLI is supported by FO4 schema")
                }
            };
        let lvlo_entry = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "LVLO")
            .expect("converted LVLO");
        let encoded =
            crate::target_write::encode_field_pub(lvlo_entry, schema.record_def("LVLI"), &interner)
                .expect("converted LVLO encodes");
        assert_eq!(encoded, vec![2, 0, 0, 0, 0x65, 0x9C, 0x83, 0, 7, 0, 0, 0]);
    }

    #[test]
    fn pre_translate_remaps_raw_fo76_caps_lvlo_to_fo4_caps_misc() {
        let mut interner = StringInterner::new();
        let mut record = make_record("LVLI", &mut interner);
        let raw_lvlo = vec![1, 0, 0, 0, 0x0F, 0, 0, 0, 100, 0, 0, 0];
        push_field(&mut record, "LLCT", FieldValue::Uint(1));
        push_field(
            &mut record,
            "LVLO",
            FieldValue::Bytes(SmallVec::from_vec(raw_lvlo)),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let lvlo_entry = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "LVLO")
            .expect("converted LVLO");
        let FieldValue::Struct(fields) = &lvlo_entry.value else {
            panic!("LVLO should be converted into an FO4 struct");
        };
        let item = named_value(fields, "item", &interner).expect("item reference");
        let FieldValue::FormKey(fk) = item else {
            panic!("LVLO item should be a typed FormKey, got {item:?}");
        };
        assert_eq!(fk.local, 0x00000F);
        assert_eq!(interner.resolve(fk.plugin), Some("Fallout4.esm"));

        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let record =
            match crate::target_normalize::TargetRecordNormalizer::target_only_with_interner(
                &schema, &interner,
            )
            .normalize(record)
            {
                crate::target_normalize::TargetRecordNormalization::Keep(record) => record,
                crate::target_normalize::TargetRecordNormalization::DropUnsupportedRecord => {
                    panic!("LVLI is supported by FO4 schema")
                }
            };
        let lvlo_entry = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "LVLO")
            .expect("converted LVLO");
        let encoded =
            crate::target_write::encode_field_pub(lvlo_entry, schema.record_def("LVLI"), &interner)
                .expect("converted LVLO encodes");
        assert_eq!(encoded, vec![1, 0, 0, 0, 0x0F, 0, 0, 0, 100, 0, 0, 0]);
    }

    #[test]
    fn pre_translate_preserves_npc_object_template_group() {
        // The full-plugin path carries the NPC Object Template (OBTE..STOP) so
        // modular robots render with their parts; post_translate's
        // strip_invalid_object_mod_properties + the raw-formid remap fixup keep
        // it FO4-safe. The cell-slice strip lives in a GraphOnly fixup instead.
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", &mut interner);
        let template_name = interner.intern("Default Template");
        let record_name = interner.intern("Thrasher");
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(
            &mut record,
            "OBTE",
            FieldValue::Bytes(SmallVec::from_vec(vec![1, 0, 0, 0])),
        );
        push_field(&mut record, "OBTF", FieldValue::Bytes(SmallVec::new()));
        push_field(&mut record, "FULL", FieldValue::String(template_name));
        push_field(
            &mut record,
            "OBTS",
            FieldValue::Bytes(SmallVec::from_vec(vec![0; 25])),
        );
        push_field(&mut record, "STOP", FieldValue::Bytes(SmallVec::new()));
        push_field(
            &mut record,
            "CNAM",
            FieldValue::Bytes(SmallVec::from_vec(vec![1, 2, 3, 4])),
        );
        push_field(&mut record, "FULL", FieldValue::String(record_name));
        push_field(&mut record, "DATA", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(
            sigs,
            vec![
                "EDID", "OBTE", "OBTF", "FULL", "OBTS", "STOP", "CNAM", "FULL", "DATA"
            ]
        );
        let full_names: Vec<&str> = record
            .fields
            .iter()
            .filter_map(|field| match &field.value {
                FieldValue::String(sym) if field.sig.0 == *b"FULL" => interner.resolve(*sym),
                _ => None,
            })
            .collect();
        assert_eq!(full_names, vec!["Default Template", "Thrasher"]);
    }

    #[test]
    fn post_translate_normalizes_npc_raw_form_refs() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", &mut interner);
        push_field(
            &mut record,
            "SNAM",
            raw_bytes(&[0x08, 0xC0, 0x3F, 0x00, 0xFE]),
        );
        push_field(
            &mut record,
            "CNTO",
            raw_bytes(&[0x84, 0xAB, 0x33, 0x00, 1, 0, 0, 0]),
        );
        push_field(&mut record, "INAM", raw_bytes(&[0x50, 0xE3, 0x04, 0x00]));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let snam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "SNAM")
            .expect("SNAM remains");
        let FieldValue::Struct(snam_fields) = &snam.value else {
            panic!("SNAM should be structured");
        };
        let FieldValue::FormKey(faction) =
            named_value(snam_fields, "faction", &interner).expect("faction")
        else {
            panic!("faction should be a FormKey");
        };
        assert_eq!(faction.local, 0x3FC008);
        assert_eq!(interner.resolve(faction.plugin), Some("SeventySix.esm"));
        assert_eq!(
            named_value(snam_fields, "rank", &interner).expect("rank"),
            &raw_bytes(&[0xFE])
        );

        let cnto = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "CNTO")
            .expect("CNTO remains");
        let FieldValue::Struct(cnto_fields) = &cnto.value else {
            panic!("CNTO should be structured");
        };
        let FieldValue::FormKey(item) = named_value(cnto_fields, "item", &interner).expect("item")
        else {
            panic!("item should be a FormKey");
        };
        assert_eq!(item.local, 0x33AB84);
        assert_eq!(interner.resolve(item.plugin), Some("SeventySix.esm"));
        assert_eq!(
            named_value(cnto_fields, "count", &interner).expect("count"),
            &raw_bytes(&[1, 0, 0, 0])
        );

        let inam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "INAM")
            .expect("INAM remains");
        let FieldValue::FormKey(death_item) = &inam.value else {
            panic!("INAM should be a FormKey");
        };
        assert_eq!(death_item.local, 0x04E350);
        assert_eq!(interner.resolve(death_item.plugin), Some("SeventySix.esm"));
    }

    #[test]
    fn post_translate_remaps_rd01_assassin_combat_style_to_ranged() {
        let mut interner = StringInterner::new();
        let fk = FormKey::parse("78BD9B@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("NPC_").unwrap(), fk);
        let eid = interner.intern("RD01_Enc04_Assassin");
        let source_plugin = interner.intern("SeventySix.esm");
        record.eid = Some(eid);
        push_field(&mut record, "EDID", FieldValue::String(eid));
        push_field(
            &mut record,
            "ZNAM",
            FieldValue::FormKey(FormKey {
                local: CS_RAIDER_01_MELEE_FORM_ID,
                plugin: source_plugin,
            }),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let combat_style = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "ZNAM")
            .expect("combat style remains");
        let FieldValue::FormKey(fk) = &combat_style.value else {
            panic!("combat style should be a FormKey");
        };
        assert_eq!(fk.local, CS_RAIDER_RANGED_FORM_ID);
        assert_eq!(fk.plugin, source_plugin);
    }

    #[test]
    fn pre_translate_converts_raw_cell_xcri() {
        let mut interner = StringInterner::new();
        let mut record = make_record("CELL", &mut interner);
        let mut raw = Vec::new();
        raw.extend_from_slice(&2_u64.to_le_bytes());
        raw.extend_from_slice(&1_u64.to_le_bytes());
        raw.extend_from_slice(&0x1111_1111_u32.to_le_bytes());
        raw.extend_from_slice(&[1, 2, 3, 4]);
        raw.extend_from_slice(&0x2222_2222_u32.to_le_bytes());
        raw.extend_from_slice(&[5, 6, 7, 8]);
        raw.extend_from_slice(&0xAABB_CCDD_u32.to_le_bytes());
        raw.extend_from_slice(&[9, 10, 11, 12]);
        raw.extend_from_slice(&0x3333_3333_u32.to_le_bytes());
        raw.extend_from_slice(&[13, 14, 15, 16]);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(
            &mut record,
            "XCRI",
            FieldValue::Bytes(SmallVec::from_vec(raw)),
        );
        push_field(&mut record, "XCLC", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["EDID", "XCRI", "XCLC"]);
        let FieldValue::Bytes(bytes) = &record.fields[1].value else {
            panic!("expected raw XCRI bytes");
        };
        let mut expected = Vec::new();
        expected.extend_from_slice(&2_u32.to_le_bytes());
        expected.extend_from_slice(&1_u32.to_le_bytes());
        expected.extend_from_slice(&0x1111_1111_u32.to_le_bytes());
        expected.extend_from_slice(&0x2222_2222_u32.to_le_bytes());
        expected.extend_from_slice(&0xAABB_CCDD_u32.to_le_bytes());
        expected.extend_from_slice(&0x3333_3333_u32.to_le_bytes());
        assert_eq!(bytes.as_slice(), expected.as_slice());
    }

    #[test]
    fn pre_translate_converts_structured_cell_xcri() {
        let mut interner = StringInterner::new();
        let mut record = make_record("CELL", &mut interner);
        let xcri = FieldValue::Struct(vec![
            (interner.intern("meshes_count"), FieldValue::Uint(1)),
            (interner.intern("references_count"), FieldValue::Uint(1)),
            (
                interner.intern("meshes"),
                FieldValue::List(vec![FieldValue::Struct(vec![
                    (interner.intern("combined_mesh"), FieldValue::Uint(7)),
                    (interner.intern("unknown_u8_1"), FieldValue::Uint(255)),
                ])]),
            ),
            (
                interner.intern("references"),
                FieldValue::List(vec![FieldValue::Struct(vec![
                    (
                        interner.intern("reference"),
                        FieldValue::Bytes(SmallVec::from_vec(
                            0x1234_5678_u32.to_le_bytes().to_vec(),
                        )),
                    ),
                    (interner.intern("unknown_u8_1"), FieldValue::Uint(255)),
                    (interner.intern("combined_mesh"), FieldValue::Uint(7)),
                ])]),
            ),
        ]);
        push_field(&mut record, "XCRI", xcri);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(record.fields.len(), 1);
        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let encoded = crate::target_write::encode_field_pub(
            &record.fields[0],
            schema.record_def("CELL"),
            &interner,
        )
        .expect("converted XCRI encodes");
        let mut expected = Vec::new();
        expected.extend_from_slice(&1_u32.to_le_bytes());
        expected.extend_from_slice(&1_u32.to_le_bytes());
        expected.extend_from_slice(&7_u32.to_le_bytes());
        expected.extend_from_slice(&0x1234_5678_u32.to_le_bytes());
        expected.extend_from_slice(&7_u32.to_le_bytes());
        assert_eq!(encoded, expected);
    }

    #[test]
    fn pre_translate_drops_malformed_cell_xcri() {
        let mut interner = StringInterner::new();
        let mut record = make_record("CELL", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(
            &mut record,
            "XCRI",
            FieldValue::Bytes(SmallVec::from_vec(vec![1, 2, 3])),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["EDID"]);
    }

    #[test]
    fn pre_translate_keeps_xcri_on_non_cell_records() {
        let mut interner = StringInterner::new();
        let mut record = make_record("STAT", &mut interner);
        push_field(
            &mut record,
            "XCRI",
            FieldValue::Bytes(SmallVec::from_vec(vec![0; 32])),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["XCRI"]);
    }

    #[test]
    fn pre_translate_keeps_qust_vmad_and_full_alias_chain() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(
            &mut record,
            "VMAD",
            FieldValue::Bytes(SmallVec::from_vec(vec![1, 2, 3])),
        );
        push_field(&mut record, "FULL", FieldValue::None);
        push_field(
            &mut record,
            "FNAM",
            FieldValue::Bytes(SmallVec::from_vec(vec![0; 4])),
        );
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(34_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALST",
            FieldValue::Bytes(SmallVec::from_vec(0_u32.to_le_bytes().to_vec())),
        );
        push_field(&mut record, "ALID", FieldValue::Bytes(SmallVec::new()));
        // FO76-only alias keyword/faction-rank fields: dropped even though they
        // appear inside the alias chain.
        push_field(&mut record, "KNAM", FieldValue::Bytes(SmallVec::new()));
        push_field(&mut record, "ALFC", FieldValue::Bytes(SmallVec::new()));
        // FO76 event alias-fill data is unsafe once the FO76 event scope is
        // stripped, so the alias row survives without these fields.
        push_field(&mut record, "ALFE", FieldValue::Bytes(SmallVec::new()));
        push_field(
            &mut record,
            "ALFD",
            FieldValue::Bytes(SmallVec::from_vec(0x00003152_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALLS",
            FieldValue::Bytes(SmallVec::from_vec(35_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALCS",
            FieldValue::Bytes(SmallVec::from_vec(36_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "KSIZ",
            FieldValue::Bytes(SmallVec::from_vec(1_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "KWDA",
            FieldValue::Bytes(SmallVec::from_vec(vec![1, 0, 0, 0])),
        );
        push_field(&mut record, "ALRT", FieldValue::Bytes(SmallVec::new()));
        push_field(&mut record, "SNAM", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(
            sigs,
            vec![
                "EDID", "VMAD", "FULL", "FNAM", "ANAM", "ALST", "ALID", "ALLS", "ALCS", "KSIZ",
                "KWDA", "ALRT", "SNAM"
            ]
        );
        match &record.fields[4].value {
            FieldValue::Bytes(bytes) => assert_eq!(&bytes[..4], &34_u32.to_le_bytes()),
            other => panic!("ANAM should retain next alias id bytes, got {other:?}"),
        }
    }

    #[test]
    fn pre_translate_marks_event_filled_qust_alias_optional_when_fill_is_stripped() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(14_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALST",
            FieldValue::Bytes(SmallVec::from_vec(13_u32.to_le_bytes().to_vec())),
        );
        push_field(&mut record, "ALID", FieldValue::Bytes(SmallVec::new()));
        push_field(
            &mut record,
            "FNAM",
            FieldValue::Bytes(SmallVec::from_vec(0_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALFE",
            FieldValue::Bytes(SmallVec::from_vec(1329742913_u32.to_le_bytes().to_vec())),
        );
        push_field(
            &mut record,
            "ALFD",
            FieldValue::Bytes(SmallVec::from_vec(1_u32.to_le_bytes().to_vec())),
        );
        push_field(&mut record, "ALED", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["EDID", "ANAM", "ALST", "ALID", "FNAM", "ALED"]);
        let fnam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "FNAM")
            .expect("alias FNAM");
        let FieldValue::Bytes(bytes) = &fnam.value else {
            panic!("FNAM should stay raw bytes");
        };
        let raw_flags = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        assert_eq!(
            raw_flags & QUST_ALIAS_OPTIONAL_FLAG,
            QUST_ALIAS_OPTIONAL_FLAG
        );
    }

    #[test]
    fn build_fo4_qust_dnam_relayouts_20_byte_flags64_variant() {
        // FO76 form_version >= 202: flags is u64 (bytes 0..8).
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8311_u64.to_le_bytes());
        data[8] = 5; // priority
        data[12..16].copy_from_slice(&1.5_f32.to_le_bytes()); // delay_time
        data[16] = 2; // quest_type

        let dnam = build_fo4_qust_dnam_from_fo76_data(&data).expect("dnam");
        assert_eq!(dnam.len(), FO4_QUST_DNAM_LEN);
        assert_eq!(u16::from_le_bytes([dnam[0], dnam[1]]), 0x8311);
        assert_eq!(dnam[0] & 0x01, 0x01, "start_game_enabled bit preserved");
        assert_eq!(dnam[2], 5, "priority");
        assert_eq!(f32::from_le_bytes(dnam[4..8].try_into().unwrap()), 1.5);
        assert_eq!(dnam[8], 2, "quest_type");
    }

    #[test]
    fn build_fo4_qust_dnam_relayouts_16_byte_flags32_variant() {
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS32_LEN];
        data[0..4].copy_from_slice(&0x0000_0111_u32.to_le_bytes());
        data[4] = 7; // priority
        data[8..12].copy_from_slice(&2.0_f32.to_le_bytes());
        data[12] = 3; // quest_type

        let dnam = build_fo4_qust_dnam_from_fo76_data(&data).expect("dnam");
        assert_eq!(u16::from_le_bytes([dnam[0], dnam[1]]), 0x0111);
        assert_eq!(dnam[2], 7);
        assert_eq!(f32::from_le_bytes(dnam[4..8].try_into().unwrap()), 2.0);
        assert_eq!(dnam[8], 3);
    }

    #[test]
    fn build_fo4_qust_dnam_masks_fo76_only_high_flag_bits() {
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        // 0x80000 = holotape_only (FO76-only) ORed with 0x8311 standard low bits.
        data[0..8].copy_from_slice(&0x0000_0000_0008_8311_u64.to_le_bytes());
        let dnam = build_fo4_qust_dnam_from_fo76_data(&data).expect("dnam");
        assert_eq!(
            u16::from_le_bytes([dnam[0], dnam[1]]),
            0x8311,
            "FO76-only high flag bits masked off"
        );
    }

    #[test]
    fn build_fo4_qust_dnam_rejects_unknown_length() {
        assert!(build_fo4_qust_dnam_from_fo76_data(&[0u8; 13]).is_none());
    }

    #[test]
    fn pre_translate_converts_qust_data_to_fo4_dnam() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8311_u64.to_le_bytes());
        data[8] = 5;
        data[16] = 2;
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"DATA"), "FO76 DATA renamed away");
        assert!(sigs.contains(&"DNAM"), "FO4 DNAM emitted");
        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM should be raw bytes");
        };
        assert_eq!(bytes.len(), FO4_QUST_DNAM_LEN);
        // Relayout is byte-for-byte EXCEPT start-game-enabled (0x0001): this
        // quest has no EditorID, so it is not a dialogue/radio quest and the
        // disable-by-default autostart policy clears SGE (0x8311 -> 0x8310).
        assert_eq!(u16::from_le_bytes([bytes[0], bytes[1]]), 0x8310);
    }

    #[test]
    fn pre_translate_qust_keeps_existing_dnam_untouched() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(
            &mut record,
            "DNAM",
            raw_bytes(&[0x01, 0x00, 9, 0, 0, 0, 0, 0, 1, 0, 0, 0]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes");
        };
        assert_eq!(bytes[2], 9, "existing DNAM priority preserved");
    }

    #[test]
    fn pre_translate_qust_data_to_dnam_ignores_non_qust() {
        let mut interner = StringInterner::new();
        let mut record = make_record("WEAP", &mut interner);
        push_field(
            &mut record,
            "DATA",
            raw_bytes(&[0u8; FO76_QUST_DATA_FLAGS64_LEN]),
        );

        Fo76Fo4Hook::convert_qust_data_to_fo4_dnam(&interner, &mut record);

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"DATA"), "non-QUST DATA left untouched");
        assert!(!sigs.contains(&"DNAM"));
    }

    #[test]
    fn force_dialogue_autostart_sets_sge_when_has_dialogue_data() {
        // DNAM flags low byte 0x00 (not SGE) + has_dialogue_data 0x8000 set.
        let mut dnam = vec![0x00, 0x80, 5, 0, 0, 0, 0, 0, 2, 0, 0, 0];
        force_dialogue_quest_autostart(&mut dnam);
        let flags = u16::from_le_bytes([dnam[0], dnam[1]]);
        assert_eq!(flags & 0x0001, 0x0001, "start_game_enabled set");
        assert_eq!(flags & 0x8000, 0x8000, "has_dialogue_data preserved");
        assert_eq!(dnam[2], 5, "priority untouched");
    }

    #[test]
    fn force_dialogue_autostart_noop_without_has_dialogue_data() {
        // has_dialogue_data NOT set -> guard refuses to force-start.
        let mut dnam = vec![0x00, 0x00, 5, 0, 0, 0, 0, 0, 2, 0, 0, 0];
        force_dialogue_quest_autostart(&mut dnam);
        assert_eq!(
            u16::from_le_bytes([dnam[0], dnam[1]]) & 0x0001,
            0,
            "SGE not added"
        );
    }

    #[test]
    fn force_dialogue_autostart_is_idempotent() {
        let mut dnam = vec![0x01, 0x80, 5, 0, 0, 0, 0, 0, 2, 0, 0, 0];
        force_dialogue_quest_autostart(&mut dnam);
        assert_eq!(u16::from_le_bytes([dnam[0], dnam[1]]), 0x8001);
    }

    #[test]
    fn qust_eid_dialogue_match_explicit_containers_only() {
        let mut interner = StringInterner::new();
        // String variant, mixed case.
        let mut r1 = make_record("QUST", &mut interner);
        push_field(
            &mut r1,
            "EDID",
            FieldValue::String(interner.intern("XPD_Dialogue_WhitespringGreeter")),
        );
        assert!(qust_eid_is_dialogue_conversation(&interner, &r1));

        // Bytes variant, lower case, NUL-terminated.
        let mut r2 = make_record("QUST", &mut interner);
        push_field(&mut r2, "EDID", raw_bytes(b"some_dialogue_thing\x00"));
        assert!(qust_eid_is_dialogue_conversation(&interner, &r2));

        let mut r3 = make_record("QUST", &mut interner);
        r3.eid = Some(interner.intern("NPCConversation_Biv"));
        assert!(qust_eid_is_dialogue_conversation(&interner, &r3));

        // Has dialogue content, but is not a dialogue-container quest.
        let mut r4 = make_record("QUST", &mut interner);
        r4.eid = Some(interner.intern("TW043"));
        assert!(!qust_eid_is_dialogue_conversation(&interner, &r4));

        // Non-dialogue gameplay quest -> no match.
        let mut r5 = make_record("QUST", &mut interner);
        push_field(
            &mut r5,
            "EDID",
            FieldValue::String(interner.intern("EN07_MQ_Nuke_Master")),
        );
        assert!(!qust_eid_is_dialogue_conversation(&interner, &r5));

        // No EDID field at all -> no match (no force-start).
        let r6 = make_record("QUST", &mut interner);
        assert!(!qust_eid_is_dialogue_conversation(&interner, &r6));
    }

    #[test]
    fn qust_eid_holotape_matches_container_not_gameplay() {
        let mut interner = StringInterner::new();
        let mut mk = |eid: &str| {
            let mut r = make_record("QUST", &mut interner);
            push_field(&mut r, "EDID", FieldValue::String(interner.intern(eid)));
            qust_eid_is_holotape(&interner, &r)
        };
        // Dedicated holotape-scene containers -> match.
        assert!(mk("HolotapeQuest_Overseer"));
        assert!(mk("HolotapeQuest_DB"));
        assert!(mk("Burn_HolotapeQuest"));
        assert!(mk("Storm_Holotapes_Cassidy"));
        assert!(mk("HolotapesQuest"));
        // Gameplay/main/misc quests that merely involve a holotape -> no match
        // (they carry a bare `*Holotape` name, not the container marker).
        assert!(!mk("Storm_MQ09_OberlinPt3_DanHolotape"));
        assert!(!mk("E06_PocketWatch_Holotape_Misc"));
        assert!(!mk("EN07_MQ_Nuke_Master"));
    }

    #[test]
    fn pre_translate_forces_holotape_container_autostart() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("HolotapeQuest_Overseer")),
        );
        // FO76 DATA flags low16 = 0x8000 (has_dialogue_data set, SGE not set).
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8000_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .expect("FO4 DNAM emitted");
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM should be raw bytes");
        };
        // Holotape container is whitelisted -> SGE forced on (0x8000 -> 0x8001).
        assert_eq!(u16::from_le_bytes([bytes[0], bytes[1]]), 0x8001);
    }

    // Build-independent ground-truth test: run the REAL source decode
    // (decode_record_from_parsed_relayout, exactly what translate_v2 Pass P
    // uses) on a real-shaped FO76 QUST, then the DNAM relayout. This catches
    // any divergence between the hand-built Record unit tests and the actual
    // decoded field shape (e.g. DATA not surfacing as Bytes(20)).
    #[test]
    fn real_decode_qust_data_then_dnam_relayout_sets_sge() {
        use crate::source_read::decode_record_from_parsed_relayout;
        use crate::struct_relayout::StructRelayoutCtx;
        use esp_authoring_core::plugin_runtime::{ParsedRecord, ParsedSubrecord};
        use smol_str::SmolStr;

        let fo76 = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        let fo4 = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let interner = StringInterner::new();
        let fk = FormKey {
            local: 0x0065_3177,
            plugin: interner.intern("SeventySix.esm"),
        };

        let mut edid = b"XPD_Dialogue_WhitespringGreeter".to_vec();
        edid.push(0);
        // Real source DATA bytes captured from SeventySix.esm 0x653177:
        // flags low16 = 0x8500 (has_dialogue_data set, SGE not set).
        let data: Vec<u8> = vec![
            0x00, 0x85, 0x80, 0x02, 0x00, 0x00, 0x00, 0x00, 0x1e, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(data.len(), 20);

        let mk = |sig: &str, d: Vec<u8>| ParsedSubrecord {
            signature: SmolStr::new(sig),
            data: bytes::Bytes::from(d),
            semantic_type: None,
        };
        let raw = ParsedRecord {
            signature: SmolStr::new("QUST"),
            form_id: 0x0065_3177,
            flags: 0,
            version_control: 0,
            form_version: Some(202),
            version2: None,
            subrecords: vec![mk("EDID", edid), mk("DATA", data)],
            raw_payload: None,
            parse_error: None,
        };

        let ctx = StructRelayoutCtx {
            target_schema: &fo4,
            target_form_version: 131,
        };
        let mut record = decode_record_from_parsed_relayout(
            &raw,
            &fk,
            &fo76,
            &[],
            "SeventySix.esm",
            None,
            false,
            &interner,
            Some(&ctx),
        )
        .expect("decode");

        let data_field = record.fields.iter().find(|f| f.sig.as_str() == "DATA");
        eprintln!(
            "DATA after real decode = {:?}",
            data_field.map(|f| &f.value)
        );

        // Drive the REAL Pass-P sequence: full pre_translate (every hook step,
        // in order) then translate (map-driven drops/transforms). This is what
        // the whole-plugin translate_v2 path runs per record.
        let translator = crate::translator::Translator::new(
            crate::translator::Game::Fo76,
            crate::translator::Game::Fo4,
        )
        .expect("translator");

        let mut ctx = crate::translator::pair_hook::PairCtx {
            interner: &interner,
        };
        translator
            .pre_translate(&mut ctx, &mut record)
            .expect("pre_translate");
        let after_pt = record.fields.iter().find(|f| f.sig.as_str() == "DNAM");
        eprintln!(
            "DNAM after full pre_translate = {:?}",
            after_pt.map(|f| &f.value)
        );

        let translated = match translator.translate(&record, &interner) {
            crate::translator::TranslateResult::Translated(r) => r,
            crate::translator::TranslateResult::Dropped { .. } => panic!("translate Dropped"),
            crate::translator::TranslateResult::Deferred(_) => panic!("translate Deferred"),
        };
        let dnam = translated.fields.iter().find(|f| f.sig.as_str() == "DNAM");
        eprintln!("DNAM after translate = {:?}", dnam.map(|f| &f.value));

        let dnam = dnam.expect("DNAM must survive full pre_translate + translate");
        let FieldValue::Bytes(b) = &dnam.value else {
            panic!("DNAM should be Bytes");
        };
        assert_eq!(
            u16::from_le_bytes([b[0], b[1]]) & 0x0001,
            0x0001,
            "start_game_enabled must be forced on for a Dialogue-named quest"
        );
    }

    #[test]
    fn pre_translate_force_starts_dialogue_named_quest() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("XPD_Dialogue_WhitespringGreeter")),
        );
        // FO76 20-byte DATA: flags u64 low word 0x8500 (has_dialogue_data, NOT SGE).
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8500_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(
            u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001,
            0x0001,
            "dialogue-named quest forced Start-Game-Enabled"
        );
    }

    #[test]
    fn pre_translate_does_not_force_start_gameplay_quest() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("RE_SceneKMK01")),
        );
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8500_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(
            u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001,
            0,
            "gameplay quest left non-SGE"
        );
    }

    #[test]
    fn pre_translate_disables_event_quest_even_when_dialogue_named() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("Dialogue_EventActivity")),
        );
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8501_u64.to_le_bytes());
        data[16] = FO76_QUST_TYPE_EVENT;
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001, 0);
    }

    #[test]
    fn pre_translate_does_not_force_start_test_dialogue_quest() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("test_VHarbison_Dialogue_Someone")),
        );
        // has_dialogue_data, NOT start-game-enabled in source.
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8500_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(
            u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001,
            0,
            "test/dev dialogue quest must NOT be force-started (scene CTD)"
        );
    }

    #[test]
    fn pre_translate_clears_sge_on_faithfully_sge_test_quest() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("TestDialogueExpressions")),
        );
        // has_dialogue_data AND start-game-enabled in source (FO76 data0=0x11 family).
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8501_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(
            u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001,
            0,
            "developer test quest must never auto-start, even if FO76 marked it SGE"
        );
    }

    #[test]
    fn pre_translate_disables_gameplay_quest_with_fo76_sge() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("RE_SceneKMK01")),
        );
        // has_dialogue_data AND start-game-enabled in source (0x8501).
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8501_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(
            u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001,
            0,
            "non-dialogue/non-radio quest must be disabled even if FO76 marked it SGE"
        );
    }

    #[test]
    fn pre_translate_preserves_radio_station_autostart() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("SQ_RadioAppalachia")),
        );
        // Real station: has_dialogue_data AND start-game-enabled in source.
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8501_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(
            u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001,
            0x0001,
            "radio station keeps its FO76 start-game-enabled flag"
        );
    }

    #[test]
    fn pre_translate_disables_high_school_pa_autostart() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("CB_HighSchoolPASystem_RadioScenes")),
        );
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8501_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(
            u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001,
            0,
            "local high-school PA loop is not a radio station and must stay off"
        );
    }

    #[test]
    fn pre_translate_does_not_enable_mq_radio_segment() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(
            &mut record,
            "EDID",
            FieldValue::String(interner.intern("W05_MQ_003P_Radio")),
        );
        // MQ radio segment: has_dialogue_data, NOT start-game-enabled in source.
        let mut data = vec![0u8; FO76_QUST_DATA_FLAGS64_LEN];
        data[0..8].copy_from_slice(&0x0000_0000_0000_8500_u64.to_le_bytes());
        push_field(&mut record, "DATA", raw_bytes(&data));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let dnam = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "DNAM")
            .unwrap();
        let FieldValue::Bytes(bytes) = &dnam.value else {
            panic!("DNAM bytes")
        };
        assert_eq!(
            u16::from_le_bytes([bytes[0], bytes[1]]) & 0x0001,
            0,
            "radio quest with no FO76 SGE (main-quest segment) stays off"
        );
    }

    #[test]
    fn pre_translate_strips_fo76_qust_event_scope() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "DNAM", raw_bytes(&[0x11, 0x03, 5, 0]));
        push_field(
            &mut record,
            "ENAM",
            FieldValue::Bytes(SmallVec::from_vec(0x434F_4C49_u32.to_le_bytes().to_vec())),
        );
        push_field(&mut record, "LNAM", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["EDID", "DNAM", "LNAM"]);
    }

    #[test]
    fn pre_translate_strips_qust_objective_targets_but_keeps_alias_chain() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "CTDA", raw_ctda(300));
        push_field(&mut record, "INDX", FieldValue::None);
        push_field(&mut record, "QSDT", FieldValue::None);
        push_field(&mut record, "CTDA", raw_ctda(300));
        push_field(&mut record, "QOBJ", FieldValue::Uint(10));
        push_field(&mut record, "FNAM", FieldValue::Uint(0));
        push_field(&mut record, "NNAM", FieldValue::None);
        push_field(
            &mut record,
            "QSTA",
            FieldValue::Bytes(SmallVec::from_vec(vec![3, 0, 0, 0])),
        );
        push_field(&mut record, "CTDA", raw_ctda(300));
        push_field(&mut record, "CIS1", FieldValue::None);
        push_field(&mut record, "CIS2", FieldValue::None);
        push_field(
            &mut record,
            "QSTA",
            FieldValue::Bytes(SmallVec::from_vec(vec![4, 0, 0, 0])),
        );
        push_field(&mut record, "QOBJ", FieldValue::Uint(20));
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(5_u32.to_le_bytes().to_vec())),
        );
        push_field(&mut record, "ALST", FieldValue::Bytes(SmallVec::new()));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        // Objective-target conditions (the QSTA-led CTDA/CIS1/CIS2 runs) are
        // stripped, but the post-ANAM alias chain (ALST and the rest) is now
        // retained so the FO4 alias table is rebuilt.
        assert_eq!(
            sigs,
            vec![
                "EDID", "CTDA", "INDX", "QSDT", "CTDA", "QOBJ", "FNAM", "NNAM", "QOBJ", "ANAM",
                "ALST",
            ]
        );
    }

    #[test]
    fn pre_translate_drops_only_objective_scope_qust_snam() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "SNAM", raw_bytes(b"Interface/Quest.swf\0"));
        push_field(&mut record, "QOBJ", FieldValue::Uint(11));
        push_field(&mut record, "FNAM", FieldValue::Uint(0));
        push_field(&mut record, "QOTM", FieldValue::None);
        push_field(&mut record, "SNAM", raw_bytes(&u16::MAX.to_le_bytes()));
        push_field(&mut record, "NNAM", FieldValue::None);
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(1_u32.to_le_bytes().to_vec())),
        );
        push_field(&mut record, "ALST", FieldValue::Bytes(SmallVec::new()));
        push_field(&mut record, "SNAM", raw_bytes(b"AliasDisplayName\0"));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let snam_values: Vec<&[u8]> = record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "SNAM")
            .map(|entry| match &entry.value {
                FieldValue::Bytes(bytes) => bytes.as_slice(),
                other => panic!("SNAM must stay Bytes, got {other:?}"),
            })
            .collect();
        assert_eq!(
            snam_values,
            vec![
                b"Interface/Quest.swf\0".as_slice(),
                b"AliasDisplayName\0".as_slice()
            ]
        );
    }

    #[test]
    fn pre_translate_keeps_qust_alias_like_sigs_before_anam() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "CTDA", FieldValue::Bytes(SmallVec::new()));
        push_field(&mut record, "FNAM", FieldValue::Bytes(SmallVec::new()));
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(vec![0; 4])),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["EDID", "CTDA", "FNAM", "ANAM"]);
    }

    #[test]
    fn pre_translate_is_noop_when_no_global_fields_present() {
        let mut interner = StringInterner::new();
        let mut record = make_record("WEAP", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "FULL", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(record.fields.len(), 2);
    }

    // -------------------------------------------------------------------------
    // Behavior 2: synthetic-source-field identification
    // -------------------------------------------------------------------------

    #[test]
    fn effects_synthetic_true_for_alch_ench_perk_spel() {
        for sig in &["ALCH", "ENCH", "PERK", "SPEL"] {
            let s = SigCode::from_str(sig).unwrap();
            assert!(
                Fo76Fo4Hook::is_effects_synthetic(s),
                "{sig} should be synthetic"
            );
        }
    }

    #[test]
    fn effects_synthetic_false_for_other_records() {
        for sig in &["WEAP", "ARMO", "NPC_", "RACE"] {
            let s = SigCode::from_str(sig).unwrap();
            assert!(
                !Fo76Fo4Hook::is_effects_synthetic(s),
                "{sig} should not be synthetic"
            );
        }
    }

    // -------------------------------------------------------------------------
    // Behavior 3: effects key routing
    // -------------------------------------------------------------------------

    #[test]
    fn translate_effects_key_reroutes_data_for_alch() {
        let record_sig = SigCode::from_str("ALCH").unwrap();
        let field_sig = SubrecordSig::from_str("DATA").unwrap();
        let route = Fo76Fo4Hook::translate_effects_key(record_sig, field_sig);
        assert!(route.is_some());
        let r = route.unwrap();
        assert_eq!(r.target_sig.as_str(), "EFID");
    }

    #[test]
    fn translate_effects_key_reroutes_efid_for_ench() {
        let record_sig = SigCode::from_str("ENCH").unwrap();
        let field_sig = SubrecordSig::from_str("EFID").unwrap();
        let route = Fo76Fo4Hook::translate_effects_key(record_sig, field_sig);
        assert!(route.is_some());
        assert_eq!(route.unwrap().target_sig.as_str(), "EFID");
    }

    #[test]
    fn translate_effects_key_reroutes_efit_for_spel() {
        let record_sig = SigCode::from_str("SPEL").unwrap();
        let field_sig = SubrecordSig::from_str("EFIT").unwrap();
        let route = Fo76Fo4Hook::translate_effects_key(record_sig, field_sig);
        assert!(route.is_some());
        assert_eq!(route.unwrap().target_sig.as_str(), "EFID");
    }

    #[test]
    fn translate_effects_key_reroutes_data_for_perk_to_data() {
        let record_sig = SigCode::from_str("PERK").unwrap();
        let field_sig = SubrecordSig::from_str("DATA").unwrap();
        let route = Fo76Fo4Hook::translate_effects_key(record_sig, field_sig);
        assert!(route.is_some());
        assert_eq!(route.unwrap().target_sig.as_str(), "DATA");
    }

    #[test]
    fn translate_effects_key_no_route_for_non_effects_record() {
        let record_sig = SigCode::from_str("WEAP").unwrap();
        let field_sig = SubrecordSig::from_str("DATA").unwrap();
        assert!(Fo76Fo4Hook::translate_effects_key(record_sig, field_sig).is_none());
    }

    #[test]
    fn translate_effects_key_no_route_for_unrelated_field_in_alch() {
        let record_sig = SigCode::from_str("ALCH").unwrap();
        let field_sig = SubrecordSig::from_str("FULL").unwrap();
        assert!(Fo76Fo4Hook::translate_effects_key(record_sig, field_sig).is_none());
    }

    fn property_row(interner: &StringInterner, property_id: u16) -> FieldValue {
        property_row_with_function_type(interner, property_id, 2)
    }

    fn property_row_with_function_type(
        interner: &StringInterner,
        property_id: u16,
        function_type: u64,
    ) -> FieldValue {
        FieldValue::Struct(vec![
            (interner.intern("value_type"), FieldValue::Uint(4)),
            (
                interner.intern("function_type"),
                FieldValue::Uint(function_type),
            ),
            (
                interner.intern("property"),
                FieldValue::Uint(property_id as u64),
            ),
            (interner.intern("value_1"), FieldValue::Uint(0)),
            (interner.intern("value_2"), FieldValue::Uint(0)),
            (interner.intern("step"), FieldValue::Float(0.0)),
        ])
    }

    fn property_ids(value: &FieldValue, interner: &StringInterner) -> Vec<u16> {
        let FieldValue::Struct(fields) = value else {
            panic!("expected struct");
        };
        let Some(FieldValue::List(properties)) = named_value(fields, "properties", interner) else {
            panic!("expected properties list");
        };
        properties
            .iter()
            .map(|property| {
                let FieldValue::Struct(row_fields) = property else {
                    panic!("expected property row struct");
                };
                field_value_to_u16(named_value(row_fields, "property", interner).unwrap())
                    .expect("property id")
            })
            .collect()
    }

    fn property_function_types(value: &FieldValue, interner: &StringInterner) -> Vec<u16> {
        let FieldValue::Struct(fields) = value else {
            panic!("expected struct");
        };
        let Some(FieldValue::List(properties)) = named_value(fields, "properties", interner) else {
            panic!("expected properties list");
        };
        properties
            .iter()
            .map(|property| {
                let FieldValue::Struct(row_fields) = property else {
                    panic!("expected property row struct");
                };
                named_value(row_fields, "function_type", interner)
                    .and_then(field_value_to_u16)
                    .unwrap_or(0)
            })
            .collect()
    }

    fn raw_property_row(property_id: u16) -> [u8; 24] {
        raw_property_row_with_function_type(property_id, 2)
    }

    fn raw_property_row_with_function_type(property_id: u16, function_type: u8) -> [u8; 24] {
        let mut row = [0; 24];
        row[0] = 4;
        row[4] = function_type;
        row[8..10].copy_from_slice(&property_id.to_le_bytes());
        row
    }

    fn raw_obts(property_ids: &[u16]) -> FieldValue {
        let mut raw = Vec::new();
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&(property_ids.len() as u32).to_le_bytes());
        raw.extend_from_slice(&[0, 0, 0, 0]);
        raw.extend_from_slice(&(-1_i16).to_le_bytes());
        raw.push(1);
        raw.push(0);
        raw.push(0);
        raw.push(0);
        for property_id in property_ids {
            raw.extend_from_slice(&raw_property_row(*property_id));
        }
        FieldValue::Bytes(smallvec::SmallVec::from_vec(raw))
    }

    fn raw_obts_property_ids(value: &FieldValue) -> Vec<u16> {
        let FieldValue::Bytes(bytes) = value else {
            panic!("expected raw OBTS bytes");
        };
        let property_count = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let property_start = 18;
        (0..property_count)
            .map(|index| {
                let offset = property_start + index * 24;
                u16::from_le_bytes(bytes[offset + 8..offset + 10].try_into().unwrap())
            })
            .collect()
    }

    fn raw_omod_data(form_type: &[u8; 4], property_ids: &[u16]) -> FieldValue {
        let rows: Vec<[u8; 24]> = property_ids
            .iter()
            .map(|property_id| raw_property_row(*property_id))
            .collect();
        raw_omod_data_rows(form_type, &rows)
    }

    fn raw_omod_data_rows(form_type: &[u8; 4], rows: &[[u8; 24]]) -> FieldValue {
        let mut raw = Vec::new();
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&(rows.len() as u32).to_le_bytes());
        raw.push(0);
        raw.push(0);
        raw.extend_from_slice(form_type);
        raw.push(0);
        raw.push(0);
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&0_u32.to_le_bytes());
        for row in rows {
            raw.extend_from_slice(row);
        }
        FieldValue::Bytes(smallvec::SmallVec::from_vec(raw))
    }

    fn raw_omod_property_ids(value: &FieldValue) -> Vec<u16> {
        let FieldValue::Bytes(bytes) = value else {
            panic!("expected raw OMOD DATA bytes");
        };
        let property_count = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let property_start = 28;
        (0..property_count)
            .map(|index| {
                let offset = property_start + index * 24;
                u16::from_le_bytes(bytes[offset + 8..offset + 10].try_into().unwrap())
            })
            .collect()
    }

    fn raw_omod_property_function_types(value: &FieldValue) -> Vec<u8> {
        let FieldValue::Bytes(bytes) = value else {
            panic!("expected raw OMOD DATA bytes");
        };
        let property_count = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let property_start = 28;
        (0..property_count)
            .map(|index| {
                let offset = property_start + index * 24;
                bytes[offset + OBJECT_MOD_PROPERTY_FUNCTION_TYPE_OFFSET]
            })
            .collect()
    }

    #[test]
    fn pre_translate_strips_tesla_cannon_receiver_base_model() {
        let interner = StringInterner::new();
        let mut record = make_record("OMOD", &interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("Weapons\\TeslaCannon\\Weapon_TeslaCannon.nif")),
        );
        push_field(&mut record, "MODT", raw_bytes(&[0; 20]));
        push_field(&mut record, "ENLT", raw_bytes(&[0; 4]));
        push_field(&mut record, "INDX", FieldValue::Uint(0));
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("form_type"),
                    FieldValue::Uint(u32::from_le_bytes(*b"WEAP") as u64),
                ),
                (
                    interner.intern("properties"),
                    FieldValue::List(vec![property_row(&interner, 34)]),
                ),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect();
        assert!(!sigs.contains(&"MODL"));
        assert!(!sigs.contains(&"MODT"));
        assert!(!sigs.contains(&"ENLT"));
        assert!(!sigs.contains(&"INDX"));
        assert!(sigs.contains(&"DATA"));
    }

    #[test]
    fn pre_translate_keeps_other_indexed_omod_models() {
        let interner = StringInterner::new();
        let mut record = make_record("OMOD", &interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("Weapons\\Other\\Receiver.nif")),
        );
        push_field(&mut record, "INDX", FieldValue::Uint(0));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert!(
            record
                .fields
                .iter()
                .any(|entry| entry.sig.as_str() == "MODL")
        );
    }

    #[test]
    fn pre_translate_strips_model_fields_from_material_omod() {
        let mut interner = StringInterner::new();
        let mut record = make_record("OMOD", &mut interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("ATX/BackPacks/Backpack_HoldAll.nif")),
        );
        push_field(
            &mut record,
            "MODT",
            FieldValue::Bytes(SmallVec::from_vec(vec![0_u8; 20])),
        );
        push_field(&mut record, "MODB", FieldValue::Float(0.0));
        push_field(&mut record, "MODF", FieldValue::Uint(0));
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("form_type"),
                    FieldValue::Uint(u32::from_le_bytes(*b"ARMO") as u64),
                ),
                (
                    interner.intern("properties"),
                    FieldValue::List(vec![property_row(&interner, 13)]),
                ),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect();
        assert!(!sigs.contains(&"MODL"));
        assert!(!sigs.contains(&"MODT"));
        assert!(!sigs.contains(&"MODB"));
        assert!(!sigs.contains(&"MODF"));
        assert!(sigs.contains(&"DATA"));
    }

    #[test]
    fn pre_translate_drops_redundant_omod_target_keyword_keeps_others() {
        let interner = StringInterner::new();
        let mut record = make_record("OMOD", &interner);
        let ma_gun = FormKey::parse("37D0B2@SeventySix.esm", &interner).unwrap();
        let keeper = FormKey::parse("0ABCDE@SeventySix.esm", &interner).unwrap();
        push_field(
            &mut record,
            "MNAM",
            FieldValue::List(vec![
                FieldValue::FormKey(ma_gun),
                FieldValue::FormKey(keeper),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let mnam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "MNAM")
            .expect("MNAM must survive while it still has a keeper entry");
        let FieldValue::List(items) = &mnam.value else {
            panic!("expected MNAM list");
        };
        let locals: Vec<u32> = items
            .iter()
            .map(|item| match item {
                FieldValue::FormKey(fk) => fk.local,
                other => panic!("expected FormKey, got {other:?}"),
            })
            .collect();
        assert_eq!(
            locals,
            vec![0xABCDE],
            "ma_Gun_Appearance must be dropped from MNAM; the keeper must remain"
        );
    }

    #[test]
    fn pre_translate_removes_emptied_omod_mnam_subrecord() {
        let interner = StringInterner::new();
        let mut record = make_record("OMOD", &interner);
        let ma_gun = FormKey::parse("37D0B2@SeventySix.esm", &interner).unwrap();
        push_field(
            &mut record,
            "MNAM",
            FieldValue::List(vec![FieldValue::FormKey(ma_gun)]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert!(
            record
                .fields
                .iter()
                .all(|entry| entry.sig.as_str() != "MNAM"),
            "an MNAM array emptied by the filter must be removed entirely"
        );
    }

    #[test]
    fn pre_translate_drops_redundant_omod_target_keyword_in_raw_bytes() {
        let interner = StringInterner::new();
        let mut record = make_record("OMOD", &interner);
        // On-disk FO76 MNAM array: ma_Gun_Appearance (0737D0B2, high byte
        // retained in raw bytes) followed by an unrelated keeper keyword.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0x0737_D0B2_u32.to_le_bytes());
        bytes.extend_from_slice(&0x0700_ABCD_u32.to_le_bytes());
        push_field(&mut record, "MNAM", raw_bytes(&bytes));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let mnam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "MNAM")
            .expect("MNAM must survive while it still has a keeper entry");
        let FieldValue::Bytes(out) = &mnam.value else {
            panic!("expected MNAM bytes");
        };
        assert_eq!(
            out.as_slice(),
            &0x0700_ABCD_u32.to_le_bytes(),
            "only the ma_Gun_Appearance row should be removed from the raw array"
        );
    }

    #[test]
    fn pre_translate_keeps_material_omod_appearance_target_keyword() {
        let interner = StringInterner::new();
        let mut record = make_record("OMOD", &interner);
        let ma_gun = FormKey::parse("37D0B2@SeventySix.esm", &interner).unwrap();
        push_field(
            &mut record,
            "MNAM",
            FieldValue::List(vec![FieldValue::FormKey(ma_gun)]),
        );
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("form_type"),
                    FieldValue::Uint(u32::from_le_bytes(*b"WEAP") as u64),
                ),
                (
                    interner.intern("properties"),
                    FieldValue::List(vec![property_row(&interner, 89)]),
                ),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let mnam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "MNAM")
            .expect("material OMOD appearance target keyword must survive for FO4 mapping");
        let FieldValue::List(items) = &mnam.value else {
            panic!("expected MNAM list");
        };
        assert_eq!(items.len(), 1);
        let FieldValue::FormKey(fk) = &items[0] else {
            panic!("expected FormKey");
        };
        assert_eq!(fk.local, 0x0037_D0B2);
    }

    #[test]
    fn pre_translate_keeps_model_fields_for_non_material_omod() {
        let mut interner = StringInterner::new();
        let mut record = make_record("OMOD", &mut interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("Armor/BackPack.nif")),
        );
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("form_type"),
                    FieldValue::Uint(u32::from_le_bytes(*b"ARMO") as u64),
                ),
                (
                    interner.intern("properties"),
                    FieldValue::List(vec![property_row(&interner, 3)]),
                ),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert!(
            record
                .fields
                .iter()
                .any(|entry| entry.sig.as_str() == "MODL")
        );
    }

    #[test]
    fn post_translate_drops_unknown_weap_object_template_property() {
        let mut interner = StringInterner::new();
        let mut record = make_record("WEAP", &mut interner);
        push_field(
            &mut record,
            "OBTS",
            FieldValue::Struct(vec![
                (interner.intern("property_count"), FieldValue::Uint(2)),
                (
                    interner.intern("properties"),
                    FieldValue::List(vec![
                        property_row(&interner, 31),
                        property_row(&interner, 103),
                    ]),
                ),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let obts = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "OBTS")
            .expect("OBTS remains");
        assert_eq!(property_ids(&obts.value, &interner), vec![31]);
        let FieldValue::Struct(fields) = &obts.value else {
            panic!("expected OBTS struct");
        };
        assert_eq!(
            field_value_to_u16(named_value(fields, "property_count", &interner).unwrap()),
            Some(1),
        );
    }

    #[test]
    fn post_translate_drops_unknown_raw_weap_object_template_property() {
        let mut interner = StringInterner::new();
        let mut record = make_record("WEAP", &mut interner);
        push_field(&mut record, "OBTS", raw_obts(&[31, 103]));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let obts = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "OBTS")
            .expect("OBTS remains");
        assert_eq!(raw_obts_property_ids(&obts.value), vec![31]);
    }

    #[test]
    fn post_translate_drops_unknown_omod_property_for_form_type() {
        let mut interner = StringInterner::new();
        let mut record = make_record("OMOD", &mut interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("form_type"),
                    FieldValue::Uint(u32::from_le_bytes(*b"ARMO") as u64),
                ),
                (interner.intern("property_count"), FieldValue::Uint(2)),
                (
                    interner.intern("properties"),
                    FieldValue::List(vec![
                        property_row(&interner, 3),
                        property_row(&interner, 31),
                    ]),
                ),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let data = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "DATA")
            .expect("DATA remains");
        assert_eq!(property_ids(&data.value, &interner), vec![3]);
        let FieldValue::Struct(fields) = &data.value else {
            panic!("expected DATA struct");
        };
        assert_eq!(
            field_value_to_u16(named_value(fields, "property_count", &interner).unwrap()),
            Some(1),
        );
    }

    #[test]
    fn post_translate_drops_unknown_raw_omod_property_for_form_type() {
        let mut interner = StringInterner::new();
        let mut record = make_record("OMOD", &mut interner);
        push_field(&mut record, "DATA", raw_omod_data(b"ARMO", &[3, 31]));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let data = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "DATA")
            .expect("DATA remains");
        assert_eq!(raw_omod_property_ids(&data.value), vec![3]);
    }

    #[test]
    fn post_translate_drops_mstt_omod_data() {
        let mut interner = StringInterner::new();
        let mut record = make_record("OMOD", &mut interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![(
                interner.intern("form_type"),
                FieldValue::Uint(u32::from_le_bytes(*b"MSTT") as u64),
            )]),
        );
        push_field(
            &mut record,
            "FULL",
            FieldValue::String(interner.intern("Nuka Victory Wallpaper")),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert!(
            record
                .fields
                .iter()
                .all(|entry| entry.sig.as_str() != "DATA"),
            "MSTT OMOD DATA must be dropped"
        );
        assert!(
            record
                .fields
                .iter()
                .any(|entry| entry.sig.as_str() == "FULL"),
            "non-DATA fields remain"
        );
    }

    #[test]
    fn post_translate_drops_raw_mstt_omod_data() {
        let mut interner = StringInterner::new();
        let mut record = make_record("OMOD", &mut interner);
        push_field(&mut record, "DATA", raw_omod_data(b"MSTT", &[3]));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert!(
            record
                .fields
                .iter()
                .all(|entry| entry.sig.as_str() != "DATA"),
            "raw MSTT OMOD DATA must be dropped"
        );
    }

    #[test]
    fn post_translate_sets_material_swap_function_type() {
        let mut interner = StringInterner::new();
        let mut record = make_record("OMOD", &mut interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (
                    interner.intern("form_type"),
                    FieldValue::Uint(u32::from_le_bytes(*b"WEAP") as u64),
                ),
                (
                    interner.intern("properties"),
                    FieldValue::List(vec![
                        property_row_with_function_type(&interner, 89, 0),
                        property_row_with_function_type(&interner, 31, 0),
                    ]),
                ),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let data = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "DATA")
            .expect("DATA remains");
        assert_eq!(property_ids(&data.value, &interner), vec![89, 31]);
        assert_eq!(property_function_types(&data.value, &interner), vec![2, 0]);
    }

    #[test]
    fn post_translate_sets_raw_material_swap_function_type() {
        let mut interner = StringInterner::new();
        let mut record = make_record("OMOD", &mut interner);
        push_field(
            &mut record,
            "DATA",
            raw_omod_data_rows(
                b"WEAP",
                &[
                    raw_property_row_with_function_type(89, 0),
                    raw_property_row_with_function_type(31, 0),
                ],
            ),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let data = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "DATA")
            .expect("DATA remains");
        assert_eq!(raw_omod_property_ids(&data.value), vec![89, 31]);
        assert_eq!(raw_omod_property_function_types(&data.value), vec![2, 0]);
    }

    #[test]
    fn post_translate_drops_raw_ctda_with_fo76_only_function_id() {
        let mut interner = StringInterner::new();
        let mut record = make_record("MGEF", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "CTDA", raw_ctda(10017));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"EDID"));
        assert!(!sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_drops_orphaned_condition_strings_with_dropped_ctda() {
        let mut interner = StringInterner::new();
        let mut record = make_record("TERM", &mut interner);
        push_field(&mut record, "BSIZ", raw_bytes(&2_u32.to_le_bytes()));
        // Body-text row 1: FO76-only function → CTDA and its CIS2 must both drop.
        push_field(&mut record, "BTXT", raw_bytes(&1_u32.to_le_bytes()));
        push_field(&mut record, "CTDA", raw_ctda(10017));
        push_field(&mut record, "CIS2", raw_bytes(b"Fo76Only\0"));
        // Body-text row 2: FO4-compatible function → CTDA and its CIS2 survive.
        push_field(&mut record, "BTXT", raw_bytes(&2_u32.to_le_bytes()));
        push_field(&mut record, "CTDA", raw_ctda(560));
        push_field(&mut record, "CIS2", raw_bytes(b"Keep\0"));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(
            sigs,
            vec!["BSIZ", "BTXT", "BTXT", "CTDA", "CIS2"],
            "a dropped CTDA must take its trailing CIS2 with it; a kept CTDA keeps its CIS2",
        );
        let cis2 = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "CIS2")
            .expect("kept CIS2 survives");
        let FieldValue::Bytes(bytes) = &cis2.value else {
            panic!("expected raw CIS2 bytes");
        };
        assert_eq!(
            bytes.as_slice(),
            b"Keep\0",
            "surviving CIS2 must be the one paired with the kept CTDA",
        );
    }

    #[test]
    fn post_translate_keeps_raw_ctda_with_fo4_function_id() {
        let mut interner = StringInterner::new();
        let mut record = make_record("MGEF", &mut interner);
        push_field(&mut record, "CTDA", raw_ctda(560));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_remaps_fo76_is_quest_active_to_get_quest_running() {
        // FO76 IsQuestActive (876) has no FO4 equivalent id (> 817) and would be
        // dropped; instead it is remapped to FO4 GetQuestRunning (56), which is
        // value-identical (`== 1`) and takes the same QUST in Parameter #1.
        let mut interner = StringInterner::new();
        let mut record = make_record("LSCR", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(876, 0x0000_FFED),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let ctda = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "CTDA")
            .expect("remapped CTDA must survive the incompatibility drop");
        let FieldValue::Bytes(bytes) = &ctda.value else {
            panic!("expected raw CTDA bytes");
        };
        let function_id = u16::from_le_bytes([bytes[8], bytes[9]]);
        assert_eq!(
            function_id, 56,
            "876 IsQuestActive should remap to 56 GetQuestRunning"
        );
        let parameter_1 = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
        assert_eq!(
            parameter_1, 0x0000_FFED,
            "quest Parameter #1 must be preserved"
        );
    }

    #[test]
    fn post_translate_remaps_fo76_current_location_exact_to_get_in_current_location() {
        let mut interner = StringInterner::new();
        let mut record = make_record("LSCR", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(
                FO76_GET_IS_CURRENT_LOCATION_EXACT_CONDITION_FUNCTION_ID,
                0x007A_8A73,
            ),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let ctda = record
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "CTDA")
            .expect("remapped CTDA must survive the incompatibility drop");
        let FieldValue::Bytes(bytes) = &ctda.value else {
            panic!("expected raw CTDA bytes");
        };
        let function_id = u16::from_le_bytes([bytes[8], bytes[9]]);
        assert_eq!(
            function_id, FO4_GET_IN_CURRENT_LOCATION_CONDITION_FUNCTION_ID,
            "844 GetIsCurrentLocationExact should remap to 359 GetInCurrentLocation"
        );
        let parameter_1 = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
        assert_eq!(
            parameter_1, 0x007A_8A73,
            "location Parameter #1 must be preserved"
        );
    }

    #[test]
    fn post_translate_drops_raw_ctda_with_fo76_function_info_parameter() {
        let mut interner = StringInterner::new();
        let mut record = make_record("MUST", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(FO76_FUNCTION_INFO_CONDITION_FUNCTION_ID, 0x0063_78CE),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_keeps_raw_ctda_with_fo76_function_info_without_parameter() {
        let mut interner = StringInterner::new();
        let mut record = make_record("MUST", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda(FO76_FUNCTION_INFO_CONDITION_FUNCTION_ID),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_drops_quest_param_ctda_with_null_parameter_1() {
        let mut interner = StringInterner::new();
        let mut record = make_record("TERM", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        // GetStage (58) with a NULL QUST Parameter #1 → xEdit "Found NULL,
        // expected QUST"; the condition can't be retargeted → drop it.
        push_field(&mut record, "CTDA", raw_ctda(58));
        push_field(&mut record, "CIS1", raw_bytes(b"alias\0"));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"EDID"));
        assert!(!sigs.contains(&"CTDA"), "null-quest CTDA must be dropped");
        assert!(
            !sigs.contains(&"CIS1"),
            "the dropped CTDA's trailing CIS1 must go with it",
        );
    }

    #[test]
    fn post_translate_keeps_quest_param_ctda_with_resolved_parameter_1() {
        let mut interner = StringInterner::new();
        let mut record = make_record("TERM", &mut interner);
        // GetStage (58) with a non-null QUST Parameter #1 → valid, keep.
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(58, 0x0001_2345),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_keeps_non_quest_param_ctda_with_null_parameter_1() {
        let mut interner = StringInterner::new();
        let mut record = make_record("TERM", &mut interner);
        // Function 560 does not take a QUST in Parameter #1, so a NULL param is
        // not a quest-target violation → keep.
        push_field(&mut record, "CTDA", raw_ctda(560));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_drops_quest_alias_run_on_ctda_on_non_quest_record() {
        let mut interner = StringInterner::new();
        let mut record = make_record("ACTI", &mut interner);
        push_field(&mut record, "EDID", FieldValue::None);
        // GetStageDone(59) with a bogus non-zero Param1 (500) and RunOn=5
        // "Quest Alias" on an ACTI: no owning quest to resolve the alias against
        // -> xEdit cannot find an alias table. Drop it.
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_run_on(59, 500, CTDA_RUN_ON_QUEST_ALIAS),
        );
        push_field(&mut record, "CIS2", raw_bytes(b"alias\0"));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"EDID"));
        assert!(
            !sigs.contains(&"CTDA"),
            "quest-alias RunOn CTDA dropped on ACTI"
        );
        assert!(!sigs.contains(&"CIS2"), "trailing CIS2 dropped with it");
    }

    #[test]
    fn post_translate_keeps_quest_alias_run_on_ctda_on_quest_record() {
        let mut interner = StringInterner::new();
        let mut record = make_record("QUST", &mut interner);
        // FO4 supports quest aliases. On a QUST-context record, xEdit resolves
        // the alias against the owning quest's alias table.
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_run_on(58, 500, CTDA_RUN_ON_QUEST_ALIAS),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(
            sigs.contains(&"CTDA"),
            "quest-context record keeps quest-alias RunOn CTDA"
        );
    }

    #[test]
    fn post_translate_keeps_get_is_alias_ref_ctda_on_quest_context_record() {
        let mut interner = StringInterner::new();
        let mut record = make_record("INFO", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(FO4_QUEST_ALIAS_PARAMETER_1_CONDITION_FUNCTION_IDS[0], 3),
        );
        push_field(&mut record, "CIS1", raw_bytes(b"alias\0"));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"CTDA"), "alias-index CTDA is kept");
        assert!(sigs.contains(&"CIS1"), "trailing CIS1 is kept with it");
    }

    #[test]
    fn post_translate_drops_get_is_alias_ref_ctda_without_quest_context() {
        let mut interner = StringInterner::new();
        let mut record = make_record("ACTI", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(FO4_QUEST_ALIAS_PARAMETER_1_CONDITION_FUNCTION_IDS[0], 3),
        );
        push_field(&mut record, "CIS1", raw_bytes(b"alias\0"));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(
            !sigs.contains(&"CTDA"),
            "contextless alias-index CTDA is dropped"
        );
        assert!(!sigs.contains(&"CIS1"), "trailing CIS1 dropped with it");
    }

    #[test]
    fn post_translate_keeps_non_quest_alias_run_on_ctda_on_non_quest_record() {
        let mut interner = StringInterner::new();
        let mut record = make_record("ACTI", &mut interner);
        // RunOn=0 "Subject" (not Quest Alias), non-quest function -> keep.
        push_field(&mut record, "CTDA", raw_ctda_with_run_on(560, 0, 0));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_maps_fo76_interior_acoustic_condition_to_fo4_interior() {
        let mut interner = StringInterner::new();
        let mut record = make_record("SNDR", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda(FO76_IS_IN_INTERIOR_ACOUSTIC_SPACE_CONDITION_FUNCTION_ID),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let ctda = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "CTDA")
            .expect("mapped CTDA remains");
        let FieldValue::Bytes(bytes) = &ctda.value else {
            panic!("expected raw CTDA bytes");
        };
        assert_eq!(
            Fo76Fo4Hook::raw_condition_function_id(bytes),
            Some(FO4_IS_IN_INTERIOR_CONDITION_FUNCTION_ID),
        );
    }

    #[test]
    fn post_translate_drops_fo76_only_raw_ctda_below_fo4_max() {
        let mut interner = StringInterner::new();
        let mut record = make_record("SNDR", &mut interner);
        push_field(&mut record, "CTDA", raw_ctda(737));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"CTDA"));
    }

    /// FO76-only condition function 596 (below FO4's 817 max) on a dialogue INFO,
    /// carrying the `$73808CE` Parameter #1 seen on BS01 Brotherhood topics. The
    /// max-id guard misses it (596 < 817), so it must be caught by the explicit
    /// FO76-only id list and the whole CTDA dropped (xEdit `<Unknown:121112782>`).
    #[test]
    fn post_translate_drops_fo76_only_function_596_ctda() {
        let mut interner = StringInterner::new();
        let mut record = make_record("INFO", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(596, 0x0738_08CE),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"CTDA"), "func=596 CTDA must be dropped");
    }

    /// Guard: function 699 carries the same `$73808CE` Parameter #1 on OTHER
    /// records but is FO4-VALID (xEdit does not flag it), so it must NOT be
    /// dropped — proving the fix targets 596 specifically, not the parameter.
    #[test]
    fn post_translate_keeps_fo4_valid_function_699_ctda() {
        let mut interner = StringInterner::new();
        let mut record = make_record("INFO", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(699, 0x0738_08CE),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(
            sigs.contains(&"CTDA"),
            "func=699 CTDA must be kept (FO4-valid)"
        );
    }

    fn workshop_cobj(interner: &StringInterner, eid: &str, bench: u32) -> Record {
        let mut record = make_record("COBJ", interner);
        record.eid = Some(interner.intern(eid));
        push_field(
            &mut record,
            "CNAM",
            FieldValue::FormKey(FormKey {
                local: 0x001000,
                plugin: interner.intern(FO76_MASTER_NAME),
            }),
        );
        push_field(
            &mut record,
            "BNAM",
            FieldValue::FormKey(FormKey {
                local: bench,
                plugin: interner.intern(FO76_MASTER_NAME),
            }),
        );
        record
    }

    fn workshop_cobj_form_key(record: &Record, sig: &[u8; 4]) -> FormKey {
        let field = record
            .fields
            .iter()
            .find(|field| &field.sig.0 == sig)
            .unwrap_or_else(|| panic!("missing {}", std::str::from_utf8(sig).unwrap()));
        match &field.value {
            FieldValue::FormKey(form_key) => *form_key,
            FieldValue::List(values) => match values.first() {
                Some(FieldValue::FormKey(form_key)) => *form_key,
                other => panic!("expected FormKey list, got {other:?}"),
            },
            other => panic!("expected FormKey, got {other:?}"),
        }
    }

    #[test]
    fn workshop_cobj_scope_accepts_only_intended_editor_ids() {
        let interner = StringInterner::new();
        for eid in [
            "workshop_co_Wall",
            "ATX_workshop_co_Lights_TrainHeadlight",
            "SCORE_S24_Workshop_CO_DriveInStatue",
        ] {
            let record = workshop_cobj(&interner, eid, FO76_WORKSHOP_CATEGORY_WALLS);
            assert!(
                Fo76Fo4Hook::is_convertible_workshop_cobj(&interner, &record),
                "{eid}"
            );
        }
        for eid in [
            "zzz_ATX_workshop_co_Wall",
            "ZZZworkshop_co_Wall",
            "co_mod_Weapon_Rifle",
            "ATX_co_mod_Weapon_Rifle",
            "SCORE_co_modScrapRecipe",
            "ATX_workshop_co_mod_Weapon_Rifle",
            "co_Clothes_Outfit",
            "ATX_co_Clothes_Outfit",
            "SCORE_co_Cloths_Outfit",
            "SCORE_workshop_co_clothes_Outfit",
            "ATX_workbench_co_NotWorkshop",
        ] {
            let record = workshop_cobj(&interner, eid, FO76_WORKSHOP_CATEGORY_WALLS);
            assert!(
                !Fo76Fo4Hook::is_convertible_workshop_cobj(&interner, &record),
                "{eid}"
            );
        }
    }

    #[test]
    fn post_translate_moves_fo76_workshop_category_to_fnam_and_sets_matching_bnam() {
        let interner = StringInterner::new();
        let mut record = workshop_cobj(
            &interner,
            "ATX_workshop_co_Lights_TrainHeadlight",
            FO76_WORKSHOP_CATEGORY_LIGHTS,
        );
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let bench = workshop_cobj_form_key(&record, b"BNAM");
        assert_eq!(bench.local, FO4_WORKSHOP_WORKBENCH_POWER);
        assert_eq!(interner.resolve(bench.plugin), Some(FO4_MASTER_NAME));
        let category = workshop_cobj_form_key(&record, b"FNAM");
        assert_eq!(category.local, FO76_WORKSHOP_CATEGORY_LIGHTS);
        assert_eq!(interner.resolve(category.plugin), Some(FO76_MASTER_NAME));
    }

    #[test]
    fn post_translate_handles_fo76_main_workshop_category_keyword() {
        let interner = StringInterner::new();
        let mut record = workshop_cobj(
            &interner,
            "ATX_workshop_co_Furniture_Generic",
            FO76_WORKSHOP_CATEGORY_MAIN_FURNITURE,
        );
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(
            workshop_cobj_form_key(&record, b"BNAM").local,
            FO4_WORKSHOP_WORKBENCH_FURNITURE
        );
        assert_eq!(
            workshop_cobj_form_key(&record, b"FNAM").local,
            FO76_WORKSHOP_CATEGORY_MAIN_FURNITURE
        );
    }

    #[test]
    fn post_translate_infers_category_for_workshop_all_type_recipe() {
        let interner = StringInterner::new();
        let mut record = workshop_cobj(
            &interner,
            "SCORE_S25_workshop_co_Structure_VinesJailCell_WallFull",
            FO76_WORKSHOP_WORKBENCH_ALL_TYPE,
        );
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(
            workshop_cobj_form_key(&record, b"BNAM").local,
            FO4_WORKSHOP_WORKBENCH_EXTERIOR
        );
        assert_eq!(
            workshop_cobj_form_key(&record, b"FNAM").local,
            FO76_WORKSHOP_CATEGORY_WALLS
        );
    }

    #[test]
    fn post_translate_keeps_non_powered_fire_lights_on_furniture_workbench() {
        let interner = StringInterner::new();
        let mut record = workshop_cobj(
            &interner,
            "workshop_co_Lights_CampFire01",
            FO76_WORKSHOP_WORKBENCH_ALL_TYPE,
        );
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(
            workshop_cobj_form_key(&record, b"BNAM").local,
            FO4_WORKSHOP_WORKBENCH_FURNITURE
        );
        assert_eq!(
            workshop_cobj_form_key(&record, b"FNAM").local,
            FO76_WORKSHOP_CATEGORY_LIGHTS
        );
    }

    #[test]
    fn post_translate_does_not_touch_excluded_workshop_recipe_family() {
        let interner = StringInterner::new();
        let mut record = workshop_cobj(
            &interner,
            "ATX_workshop_co_mod_Weapon",
            FO76_WORKSHOP_CATEGORY_LIGHTS,
        );
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(
            workshop_cobj_form_key(&record, b"BNAM").local,
            FO76_WORKSHOP_CATEGORY_LIGHTS
        );
        assert!(record.fields.iter().all(|field| field.sig.0 != *b"FNAM"));
    }

    #[test]
    fn post_translate_does_not_expose_workshop_recipe_without_created_object() {
        let interner = StringInterner::new();
        let mut record = workshop_cobj(
            &interner,
            "SCORE_S25_workshop_co_Structure_VinesJailCell_WallFull",
            FO76_WORKSHOP_CATEGORY_WALLS,
        );
        record.fields.retain(|field| field.sig.0 != *b"CNAM");
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(
            workshop_cobj_form_key(&record, b"BNAM").plugin,
            interner.intern(FO76_MASTER_NAME)
        );
        assert!(record.fields.iter().all(|field| field.sig.0 != *b"FNAM"));
    }

    #[test]
    fn post_translate_drops_cobj_raw_ctda_with_ck_rejected_cell_parameter() {
        let mut interner = StringInterner::new();
        let mut record = make_record("COBJ", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(
                FO4_COBJ_EXTERIOR_CELL_REJECTED_CONDITION_FUNCTION_ID,
                0x0000_DC58,
            ),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_keeps_cobj_raw_ctda_without_cell_parameter() {
        let mut interner = StringInterner::new();
        let mut record = make_record("COBJ", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda(FO4_COBJ_EXTERIOR_CELL_REJECTED_CONDITION_FUNCTION_ID),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_keeps_non_cobj_raw_ctda_with_same_cell_parameter() {
        let mut interner = StringInterner::new();
        let mut record = make_record("MGEF", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            raw_ctda_with_parameter_1(
                FO4_COBJ_EXTERIOR_CELL_REJECTED_CONDITION_FUNCTION_ID,
                0x0000_DC58,
            ),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_drops_structured_ctda_with_fo76_only_function_id() {
        let mut interner = StringInterner::new();
        let mut record = make_record("MGEF", &mut interner);
        push_field(
            &mut record,
            "CTDA",
            FieldValue::Struct(vec![(interner.intern("Function"), FieldValue::Uint(10017))]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_drops_structured_ctda_with_fo76_function_info_parameter() {
        let mut interner = StringInterner::new();
        let mut record = make_record("MUST", &mut interner);
        let variant = interner.intern("variant");
        let value = interner.intern("value");
        push_field(
            &mut record,
            "CTDA",
            FieldValue::Struct(vec![
                (
                    interner.intern("Function"),
                    FieldValue::Uint(FO76_FUNCTION_INFO_CONDITION_FUNCTION_ID as u64),
                ),
                (
                    interner.intern("Parameter1"),
                    FieldValue::Struct(vec![
                        (variant, FieldValue::String(interner.intern("base_object"))),
                        (value, FieldValue::Uint(0x0063_78CE)),
                    ]),
                ),
            ]),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(!sigs.contains(&"CTDA"));
    }

    #[test]
    fn post_translate_strips_pack_conditions_and_resets_condition_counts() {
        let interner = StringInterner::new();
        let mut record = make_record("PACK", &interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(&mut record, "XNAM", raw_bytes(&[0x0D]));
        push_field(&mut record, "ANAM", raw_bytes(b"Procedure\0"));
        push_field(&mut record, "CITC", raw_bytes(&2_u32.to_le_bytes()));
        push_field(&mut record, "CTDA", raw_ctda(0x012C));
        push_field(&mut record, "CIS1", raw_bytes(b"alias\0"));
        push_field(&mut record, "CTDT", raw_ctda(0x0190));
        push_field(&mut record, "CIS2", raw_bytes(b"function\0"));
        push_field(&mut record, "PNAM", raw_bytes(b"Trav"));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"EDID"));
        assert!(sigs.contains(&"PNAM"));
        assert!(!sigs.contains(&"CTDA"));
        assert!(!sigs.contains(&"CTDT"));
        assert!(!sigs.contains(&"CIS1"));
        assert!(!sigs.contains(&"CIS2"));

        let citc = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "CITC")
            .expect("CITC remains");
        assert_eq!(citc.value, raw_bytes(&0_u32.to_le_bytes()));
    }

    #[test]
    fn post_translate_preserves_pack_location_and_target_references() {
        let interner = StringInterner::new();
        let mut record = make_record("PACK", &interner);
        push_field(&mut record, "PKCU", raw_bytes(&[0; 12]));
        push_field(&mut record, "PLDT", raw_bytes(&[0, 0x52, 0x7D, 0x52, 0]));
        push_field(&mut record, "PTDA", raw_bytes(&[0, 0x52, 0x7D, 0x52, 0]));
        push_field(&mut record, "CNAM", raw_bytes(&[1, 2, 3, 4]));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"PKCU"));
        assert!(sigs.contains(&"CNAM"));
        assert!(sigs.contains(&"PLDT"));
        assert!(sigs.contains(&"PTDA"));
    }

    fn pldt_bytes(type_value: i32, location_value: u32) -> FieldValue {
        let mut raw = Vec::with_capacity(16);
        raw.extend_from_slice(&type_value.to_le_bytes());
        raw.extend_from_slice(&location_value.to_le_bytes());
        raw.extend_from_slice(&(-1i32).to_le_bytes()); // Radius
        raw.extend_from_slice(&0u32.to_le_bytes()); // Collection Index
        raw_bytes(&raw)
    }

    fn ptda_bytes(type_value: i32, target_value: u32) -> FieldValue {
        let mut raw = Vec::with_capacity(12);
        raw.extend_from_slice(&type_value.to_le_bytes());
        raw.extend_from_slice(&target_value.to_le_bytes());
        raw.extend_from_slice(&1i32.to_le_bytes()); // Count / Distance
        raw_bytes(&raw)
    }

    #[test]
    fn post_translate_neutralizes_dangling_pack_alias_location_and_target() {
        let interner = StringInterner::new();
        let mut record = make_record("PACK", &interner);
        push_field(&mut record, "PKCU", raw_bytes(&[0; 12]));
        // PLDT Type 8 (Ref Alias) -> alias index 34 cannot be proven valid here.
        push_field(&mut record, "PLDT", pldt_bytes(8, 34));
        // PTDA Type 4 (Alias) -> alias index 12 dangles.
        push_field(&mut record, "PTDA", ptda_bytes(4, 12));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let pldt = record
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "PLDT")
            .expect("PLDT survives");
        let FieldValue::Bytes(b) = &pldt.value else {
            panic!("PLDT bytes");
        };
        assert_eq!(b.len(), 16, "PLDT length unchanged");
        assert_eq!(
            i32::from_le_bytes(b[0..4].try_into().unwrap()),
            2,
            "Type -> Near Package Start"
        );
        assert_eq!(
            u32::from_le_bytes(b[4..8].try_into().unwrap()),
            0,
            "dangling alias index zeroed"
        );

        let ptda = record
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "PTDA")
            .expect("PTDA survives");
        let FieldValue::Bytes(b) = &ptda.value else {
            panic!("PTDA bytes");
        };
        assert_eq!(b.len(), 12, "PTDA length unchanged");
        assert_eq!(
            i32::from_le_bytes(b[0..4].try_into().unwrap()),
            6,
            "Type -> Self"
        );
        assert_eq!(
            u32::from_le_bytes(b[4..8].try_into().unwrap()),
            0,
            "dangling alias index zeroed"
        );
    }

    #[test]
    fn post_translate_leaves_non_alias_pack_location_types_untouched() {
        let interner = StringInterner::new();
        let mut record = make_record("PACK", &interner);
        push_field(&mut record, "PKCU", raw_bytes(&[0; 12]));
        // PLDT Type 0 (Reference) carries a FormID, not an alias — must survive verbatim.
        push_field(&mut record, "PLDT", pldt_bytes(0, 0x0001_2345));
        // PTDA Type 1 (Object ID) — not an alias.
        push_field(&mut record, "PTDA", ptda_bytes(1, 0x0006_789A));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let pldt = record
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "PLDT")
            .unwrap();
        assert_eq!(
            pldt.value,
            pldt_bytes(0, 0x0001_2345),
            "non-alias PLDT untouched"
        );
        let ptda = record
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "PTDA")
            .unwrap();
        assert_eq!(
            ptda.value,
            ptda_bytes(1, 0x0006_789A),
            "non-alias PTDA untouched"
        );
    }

    #[test]
    fn post_translate_neutralizes_dangling_pack_alias_in_plvd_location() {
        let interner = StringInterner::new();
        let mut record = make_record("PACK", &interner);
        // PLVD Type 14 (Ref Collection Alias) -> dangling.
        push_field(&mut record, "PLVD", pldt_bytes(14, 7));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let plvd = record
            .fields
            .iter()
            .find(|e| e.sig.as_str() == "PLVD")
            .unwrap();
        let FieldValue::Bytes(b) = &plvd.value else {
            panic!("PLVD bytes");
        };
        assert_eq!(i32::from_le_bytes(b[0..4].try_into().unwrap()), 2);
        assert_eq!(u32::from_le_bytes(b[4..8].try_into().unwrap()), 0);
    }

    #[test]
    fn post_translate_maps_pack_fallback_procedure_to_sequence() {
        let interner = StringInterner::new();
        let mut record = make_record("PACK", &interner);
        push_field(&mut record, "PKCU", raw_bytes(&[0; 12]));
        push_field(&mut record, "ANAM", raw_bytes(b"Fallback\0"));
        push_field(&mut record, "XNAM", raw_bytes(&[0]));
        push_field(&mut record, "ANAM", raw_bytes(b"Fallback\0"));
        push_field(
            &mut record,
            "ANAM",
            FieldValue::String(interner.intern("Fallback")),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let anams: Vec<String> = record
            .fields
            .iter()
            .filter(|field| matches!(field.sig.as_str(), "ANAM" | "PNAM"))
            .map(|field| match &field.value {
                FieldValue::Bytes(bytes) => {
                    let value = bytes
                        .as_slice()
                        .strip_suffix(&[0])
                        .unwrap_or(bytes.as_slice());
                    String::from_utf8(value.to_vec()).unwrap()
                }
                FieldValue::String(sym) => interner.resolve(*sym).unwrap().to_string(),
                other => panic!("expected procedure string-ish value, got {other:?}"),
            })
            .collect();
        assert_eq!(anams, vec!["Fallback", "Sequence", "Sequence"]);
    }

    #[test]
    fn post_translate_rewrites_fo76_pack_procedure_tree_for_fo4() {
        let interner = StringInterner::new();
        let mut record = make_record("PACK", &interner);
        push_field(&mut record, "PKCU", raw_bytes(&[0; 12]));
        push_field(&mut record, "XNAM", raw_bytes(&[0x0D]));
        push_field(&mut record, "ANAM", raw_bytes(b"Stacked\0"));
        push_field(&mut record, "CITC", FieldValue::Uint(0));
        push_field(&mut record, "PRCB", raw_bytes(&[1, 0, 0, 0, 0, 0, 0, 0]));
        push_field(&mut record, "PNAM", raw_bytes(b"Foll"));
        push_field(&mut record, "FNAM", FieldValue::Uint(0));
        push_field(&mut record, "PKC2", FieldValue::Uint(0));
        push_field(&mut record, "ANAM", raw_bytes(b"Procedure\0"));
        push_field(&mut record, "CITC", FieldValue::Uint(0));
        push_field(&mut record, "PNAM", raw_bytes(&[1, 0, 0, 0]));
        push_field(&mut record, "PKC2", FieldValue::Uint(1));
        push_field(&mut record, "UNAM", FieldValue::Uint(0));
        push_field(&mut record, "BNAM", raw_bytes(b"target\0"));
        push_field(&mut record, "PNAM", raw_bytes(&[1, 0, 0, 0]));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(
            sigs,
            vec![
                "PKCU", "XNAM", "ANAM", "CITC", "PRCB", "ANAM", "CITC", "PNAM", "FNAM", "PKC2",
                "PKC2", "UNAM", "BNAM", "PNAM"
            ]
        );
        let procedure_names: Vec<String> = record
            .fields
            .iter()
            .take_while(|field| field.sig.as_str() != "UNAM")
            .filter(|field| field.sig.as_str() == "PNAM")
            .map(|field| match &field.value {
                FieldValue::Bytes(bytes) => {
                    String::from_utf8(trim_nul_suffix(bytes.as_slice()).to_vec()).unwrap()
                }
                FieldValue::String(sym) => interner.resolve(*sym).unwrap().to_string(),
                other => panic!("expected procedure name, got {other:?}"),
            })
            .collect();
        assert_eq!(procedure_names, vec!["Follow"]);
    }

    #[test]
    fn post_translate_preserves_valid_fo4_procedures_with_long_names() {
        // FO76 packages carrying procedures whose names are valid FO4 procedures
        // (GuardArea, Hover, KeepAnEyeOn, LockDoors, UseMagic, Acquire, FollowTo)
        // must survive the procedure-tree rewrite, not be dropped.
        // Values are interned strings, matching the schema-decoded production form.
        let interner = StringInterner::new();
        let mut record = make_record("PACK", &interner);
        let s = |v: &str| FieldValue::String(interner.intern(v));
        push_field(&mut record, "PKCU", raw_bytes(&[0; 12]));
        push_field(&mut record, "XNAM", raw_bytes(&[0x0D]));
        push_field(&mut record, "ANAM", s("Simultaneous"));
        push_field(&mut record, "CITC", FieldValue::Uint(0));
        push_field(&mut record, "PRCB", raw_bytes(&[1, 0, 0, 0, 0, 0, 0, 0]));
        push_field(&mut record, "ANAM", s("Procedure"));
        push_field(&mut record, "CITC", FieldValue::Uint(0));
        push_field(&mut record, "PNAM", s("GuardArea"));
        push_field(&mut record, "FNAM", FieldValue::Uint(0));
        push_field(&mut record, "PKC2", FieldValue::Uint(0));
        push_field(&mut record, "ANAM", s("Procedure"));
        push_field(&mut record, "CITC", FieldValue::Uint(0));
        push_field(&mut record, "PNAM", s("Sandbox"));
        push_field(&mut record, "FNAM", FieldValue::Uint(0));
        push_field(&mut record, "PKC2", FieldValue::Uint(0));
        push_field(&mut record, "UNAM", FieldValue::Uint(0));
        push_field(&mut record, "BNAM", s("target"));
        push_field(&mut record, "PNAM", raw_bytes(&[1, 0, 0, 0]));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let proc_names: Vec<String> = record
            .fields
            .iter()
            .take_while(|f| f.sig.as_str() != "UNAM")
            .filter(|f| f.sig.as_str() == "PNAM")
            .map(|f| match &f.value {
                FieldValue::Bytes(b) => {
                    String::from_utf8(trim_nul_suffix(b.as_slice()).to_vec()).unwrap()
                }
                FieldValue::String(sym) => interner.resolve(*sym).unwrap().to_string(),
                other => panic!("expected procedure name, got {other:?}"),
            })
            .collect();
        assert_eq!(
            proc_names,
            vec!["GuardArea".to_string(), "Sandbox".to_string()],
            "GuardArea is a valid FO4 procedure and must be preserved, not dropped"
        );

        // Each long-name procedure maps to itself.
        for name in [
            "GuardArea",
            "Hover",
            "KeepAnEyeOn",
            "LockDoors",
            "UseMagic",
            "Acquire",
            "FollowTo",
        ] {
            let entry = FieldEntry {
                sig: SubrecordSig::from_str("PNAM").unwrap(),
                value: FieldValue::String(interner.intern(name)),
            };
            assert_eq!(
                Fo76Fo4Hook::fo76_pack_procedure_name(&interner, &entry),
                Some(name),
                "{name} must map to itself"
            );
        }
    }

    #[test]
    fn post_translate_maps_observed_fo76_pack_procedure_codes() {
        let interner = StringInterner::new();
        let names = [
            (b"Trav".as_slice(), "Travel"),
            (b"Rang".as_slice(), "Range"),
            (b"Unlo".as_slice(), "UnlockDoors"),
            (b"Hold".as_slice(), "HoldPosition"),
            (b"Say\0".as_slice(), "ForceGreet"),
            (b"UseI".as_slice(), "UseIdleMarker"),
        ];

        for (raw, expected) in names {
            let mut entry = FieldEntry {
                sig: SubrecordSig::from_str("PNAM").unwrap(),
                value: raw_bytes(raw),
            };
            let mapped =
                Fo76Fo4Hook::fo76_pack_procedure_name(&interner, &entry).expect("mapped procedure");
            assert_eq!(mapped, expected);
            Fo76Fo4Hook::set_pack_tree_text_value(&interner, &mut entry, mapped);
            match entry.value {
                FieldValue::String(sym) => {
                    assert_eq!(interner.resolve(sym), Some(expected));
                }
                other => panic!("expected interned string, got {other:?}"),
            }
        }
    }

    #[test]
    fn pre_translate_converts_fo76_mgef_data_to_fo4_layout() {
        let interner = StringInterner::new();
        let mut record = make_record("MGEF", &interner);
        let mut data = vec![0_u8; FO76_MGEF_DATA_LEN];
        data[0..4].copy_from_slice(&0xAABBCCDD_u32.to_le_bytes());
        data[4..8].copy_from_slice(&0x11223344_u32.to_le_bytes());
        data[68..72].copy_from_slice(&36_u32.to_le_bytes());
        data[72..76].copy_from_slice(&0x00000823_u32.to_le_bytes());
        data[140..144].copy_from_slice(&0x00110839_u32.to_le_bytes());
        data[156..160].copy_from_slice(&0xDEADBEEF_u32.to_le_bytes());
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(SmallVec::from_vec(data)),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.pre_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw DATA bytes");
        };
        assert_eq!(bytes.len(), FO4_MGEF_DATA_LEN);
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0xAABBCCDD
        );
        assert_eq!(u32::from_le_bytes(bytes[64..68].try_into().unwrap()), 36);
        assert_eq!(
            u32::from_le_bytes(bytes[68..72].try_into().unwrap()),
            0x00000823
        );
        assert_eq!(
            u32::from_le_bytes(bytes[136..140].try_into().unwrap()),
            0x00110839
        );
    }

    #[test]
    fn pre_translate_normalizes_fo76_only_mgef_archetypes() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);

        for (source, expected) in [
            (FO76_MGEF_ARCHETYPE_TURBO_FERT, FO4_MGEF_ARCHETYPE_SCRIPT),
            (
                FO76_MGEF_ARCHETYPE_CORPSE_HIGHLIGHT,
                FO4_MGEF_ARCHETYPE_SCRIPT,
            ),
            (FO76_MGEF_ARCHETYPE_STUN, FO4_MGEF_ARCHETYPE_STAGGER),
            (0, 0),
            (FO4_MAX_MGEF_ARCHETYPE, FO4_MAX_MGEF_ARCHETYPE),
            (0x07000814, FO4_MGEF_ARCHETYPE_SCRIPT),
        ] {
            let mut record = make_record("MGEF", &interner);
            let mut data = vec![0_u8; FO4_MGEF_DATA_LEN];
            data[FO4_MGEF_DATA_ARCHETYPE_OFFSET..FO4_MGEF_DATA_ARCHETYPE_OFFSET + 4]
                .copy_from_slice(&source.to_le_bytes());
            push_field(
                &mut record,
                "DATA",
                FieldValue::Bytes(SmallVec::from_vec(data)),
            );

            hook.pre_translate(&mut ctx, &mut record).unwrap();

            let FieldValue::Bytes(bytes) = &record.fields[0].value else {
                panic!("expected raw DATA bytes");
            };
            assert_eq!(
                u32::from_le_bytes(
                    bytes[FO4_MGEF_DATA_ARCHETYPE_OFFSET..FO4_MGEF_DATA_ARCHETYPE_OFFSET + 4]
                        .try_into()
                        .unwrap()
                ),
                expected
            );
        }
    }

    #[test]
    fn post_translate_trims_fo76_workbench_data_to_fo4_size() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);

        for record_sig in ["FURN", "TERM"] {
            let mut record = make_record(record_sig, &interner);
            push_field(
                &mut record,
                "WBDT",
                FieldValue::Bytes(SmallVec::from_vec(vec![7, 1])),
            );

            hook.post_translate(&mut ctx, &mut record).unwrap();

            let FieldValue::Bytes(bytes) = &record.fields[0].value else {
                panic!("expected WBDT bytes");
            };
            assert_eq!(bytes.as_slice(), &[7, 1]);
        }
    }

    #[test]
    fn post_translate_clears_invalid_marker_bits_but_keeps_model_backed_point_0() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);

        // Has Model (0x40000000) is set: Interaction Point 0 is backed by the
        // model's default furniture marker, so it survives even without explicit
        // marker subrecords; the higher (invalid) interaction points still clear.
        for record_sig in ["FURN", "TERM"] {
            let mut record = make_record(record_sig, &interner);
            push_field(
                &mut record,
                "MNAM",
                raw_bytes(&0x4000_001F_u32.to_le_bytes()),
            );

            hook.post_translate(&mut ctx, &mut record).unwrap();

            let FieldValue::Bytes(bytes) = &record.fields[0].value else {
                panic!("expected MNAM bytes");
            };
            assert_eq!(
                u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
                0x4000_0001,
                "model-backed Interaction Point 0 must survive; invalid points clear",
            );
        }
    }

    #[test]
    fn post_translate_clears_all_marker_bits_without_model_or_markers() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);

        // No Has Model flag and no marker subrecords: nothing backs any
        // interaction point, so all of them clear.
        for record_sig in ["FURN", "TERM"] {
            let mut record = make_record(record_sig, &interner);
            push_field(
                &mut record,
                "MNAM",
                raw_bytes(&0x0000_001F_u32.to_le_bytes()),
            );

            hook.post_translate(&mut ctx, &mut record).unwrap();

            let FieldValue::Bytes(bytes) = &record.fields[0].value else {
                panic!("expected MNAM bytes");
            };
            assert_eq!(
                u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
                0x0000_0000
            );
        }
    }

    #[test]
    fn post_translate_keeps_furniture_marker_bits_with_target_markers() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("FURN", &interner);
        push_field(
            &mut record,
            "MNAM",
            raw_bytes(&0x4000_0007_u32.to_le_bytes()),
        );
        push_field(
            &mut record,
            "SNAM",
            raw_bytes(&[0_u8; FURNITURE_MARKER_PARAMETERS_ROW_LEN * 2]),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected MNAM bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x4000_0003
        );
    }

    #[test]
    fn post_translate_adds_terminal_player_path_keyword_once() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let fo4 = interner.intern(FO4_MASTER_NAME);
        let mut record = make_record("TERM", &interner);
        push_field(
            &mut record,
            "MNAM",
            raw_bytes(&(FURNITURE_HAS_MODEL_BIT | 1).to_le_bytes()),
        );
        push_field(&mut record, "KSIZ", FieldValue::Uint(1));
        push_field(
            &mut record,
            "KWDA",
            FieldValue::List(vec![FieldValue::FormKey(FormKey {
                local: FO4_POWER_ARMOR_FIRST_PERSON_KEYWORD,
                plugin: fo4,
            })]),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let ksiz = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "KSIZ")
            .expect("terminal should retain KSIZ");
        assert_eq!(ksiz.value, FieldValue::Uint(2));
        let kwda = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "KWDA")
            .expect("terminal should retain KWDA");
        let FieldValue::List(keywords) = &kwda.value else {
            panic!("KWDA should remain a FormKey list");
        };
        assert_eq!(keywords.len(), 2);
        assert!(fo4_keyword_value(
            &kwda.value,
            &interner,
            FO4_PLAYER_PATH_TO_FURNITURE_KEYWORD,
        ));
    }

    #[test]
    fn post_translate_adds_terminal_keyword_block_when_missing() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("TERM", &interner);
        push_field(
            &mut record,
            "MNAM",
            raw_bytes(&(FURNITURE_HAS_MODEL_BIT | 1).to_le_bytes()),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["MNAM", "KSIZ", "KWDA"]);
        assert_eq!(record.fields[1].value, FieldValue::Uint(1));
        assert!(fo4_keyword_value(
            &record.fields[2].value,
            &interner,
            FO4_PLAYER_PATH_TO_FURNITURE_KEYWORD,
        ));
    }

    #[test]
    fn post_translate_adds_power_armor_battery_script_and_keeps_markers() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let fo4 = interner.intern(FO4_MASTER_NAME);
        let mut record = make_record("FURN", &interner);
        push_field(
            &mut record,
            "KWDA",
            FieldValue::List(vec![FieldValue::FormKey(FormKey {
                local: FO4_POWER_ARMOR_FURNITURE_KEYWORD,
                plugin: fo4,
            })]),
        );
        push_field(
            &mut record,
            "MNAM",
            raw_bytes(&0x4000_0003_u32.to_le_bytes()),
        );
        push_field(
            &mut record,
            "SNAM",
            raw_bytes(&[0_u8; FURNITURE_MARKER_PARAMETERS_ROW_LEN * 2]),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let vmad = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "VMAD")
            .expect("power armor furniture should have VMAD");
        let FieldValue::Bytes(bytes) = &vmad.value else {
            panic!("VMAD should be raw bytes");
        };
        let (script_name, properties) = read_power_armor_vmad(bytes);
        assert_eq!(script_name, POWER_ARMOR_BATTERY_INSERT_SCRIPT);
        assert_eq!(
            properties,
            vec![
                (
                    "firstPersonKW".to_string(),
                    FO4_POWER_ARMOR_FIRST_PERSON_KEYWORD
                ),
                (
                    "batteryInsertAnimKW".to_string(),
                    FO4_POWER_ARMOR_BATTERY_INSERT_ANIM_KEYWORD,
                ),
                (
                    "PlayerPathToFurniture".to_string(),
                    FO4_PLAYER_PATH_TO_FURNITURE_KEYWORD,
                ),
                (
                    "batteryItemKW".to_string(),
                    FO4_POWER_ARMOR_BATTERY_ITEM_KEYWORD
                ),
                (
                    "powerArmorFurnitureKW".to_string(),
                    FO4_POWER_ARMOR_FURNITURE_KEYWORD,
                ),
            ]
        );

        let mnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "MNAM")
            .expect("MNAM");
        let FieldValue::Bytes(bytes) = &mnam.value else {
            panic!("MNAM should be raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x4000_0003
        );
    }

    #[test]
    fn post_translate_recognizes_raw_power_armor_furniture_keywords() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("FURN", &interner);
        push_field(
            &mut record,
            "KWDA",
            raw_bytes(&FO4_POWER_ARMOR_FURNITURE_KEYWORD.to_le_bytes()),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert!(
            record
                .fields
                .iter()
                .any(|field| field.sig.as_str() == "VMAD")
        );
    }

    #[test]
    fn post_translate_projects_fo76_damage_type_rows_to_fo4_ck_size() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);

        for record_sig in ["ARMO", "WEAP"] {
            let mut raw = Vec::new();
            raw.extend_from_slice(&0x0102_0304_u32.to_le_bytes());
            raw.extend_from_slice(&11_u32.to_le_bytes());
            raw.extend_from_slice(&0xA0A1_A2A3_u32.to_le_bytes());
            raw.extend_from_slice(&0x0506_0708_u32.to_le_bytes());
            raw.extend_from_slice(&22_u32.to_le_bytes());
            raw.extend_from_slice(&0xB0B1_B2B3_u32.to_le_bytes());

            let mut record = make_record(record_sig, &interner);
            push_field(
                &mut record,
                "DAMA",
                FieldValue::Bytes(SmallVec::from_vec(raw)),
            );

            hook.post_translate(&mut ctx, &mut record).unwrap();

            let FieldValue::Bytes(bytes) = &record.fields[0].value else {
                panic!("expected DAMA bytes");
            };
            assert_eq!(bytes.len(), 16);
            assert_eq!(&bytes[0..4], &0x0102_0304_u32.to_le_bytes());
            assert_eq!(&bytes[4..8], &11_u32.to_le_bytes());
            assert_eq!(&bytes[8..12], &0x0506_0708_u32.to_le_bytes());
            assert_eq!(&bytes[12..16], &22_u32.to_le_bytes());
        }
    }

    #[test]
    fn post_translate_trims_fo76_movement_speed_data_to_fo4_ck_size() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("MOVT", &interner);
        let raw = (0_u8..124).collect::<Vec<_>>();
        push_field(
            &mut record,
            "SPED",
            FieldValue::Bytes(SmallVec::from_vec(raw)),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected SPED bytes");
        };
        assert_eq!(bytes.len(), FO4_MOVEMENT_SPEED_DATA_LEN);
        assert_eq!(bytes.as_slice(), (0_u8..112).collect::<Vec<_>>().as_slice());
    }

    #[test]
    fn post_translate_sets_missing_raw_light_radius_from_value() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("LIGH", &interner);
        let mut raw = vec![0_u8; 68];
        raw[FO76_LIGH_DATA_VALUE_OFFSET..FO76_LIGH_DATA_VALUE_OFFSET + 4]
            .copy_from_slice(&400_u32.to_le_bytes());
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(SmallVec::from_vec(raw)),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw DATA bytes");
        };
        assert_eq!(
            u32::from_le_bytes(
                bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            400
        );
        assert_eq!(bytes.len(), FO4_LIGH_DATA_LEN);
        assert_eq!(
            f32::from_le_bytes(
                bytes[FO4_LIGH_DATA_SCALAR_OFFSET..FO4_LIGH_DATA_SCALAR_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            FO4_LIGH_DEFAULT_SCALAR
        );
        assert_eq!(
            f32::from_le_bytes(
                bytes[FO4_LIGH_DATA_EXPONENT_OFFSET..FO4_LIGH_DATA_EXPONENT_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            FO4_LIGH_DEFAULT_EXPONENT
        );
        assert_eq!(
            u32::from_le_bytes(
                bytes[FO4_LIGH_DATA_VALUE_OFFSET..FO4_LIGH_DATA_VALUE_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            FO4_LIGH_DEFAULT_VALUE
        );
        assert_eq!(
            f32::from_le_bytes(
                bytes[FO4_LIGH_DATA_WEIGHT_OFFSET..FO4_LIGH_DATA_WEIGHT_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            FO4_LIGH_DEFAULT_WEIGHT
        );
    }

    #[test]
    fn post_translate_keeps_existing_positive_raw_light_radius() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("LIGH", &interner);
        let mut raw = vec![0_u8; 64];
        raw[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
            .copy_from_slice(&128_u32.to_le_bytes());
        raw[FO76_LIGH_DATA_VALUE_OFFSET..FO76_LIGH_DATA_VALUE_OFFSET + 4]
            .copy_from_slice(&400_u32.to_le_bytes());
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(SmallVec::from_vec(raw)),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw DATA bytes");
        };
        assert_eq!(
            u32::from_le_bytes(
                bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            128
        );
    }

    #[test]
    fn post_translate_clamps_missing_raw_light_radius_from_large_value() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("LIGH", &interner);
        let mut raw = vec![0_u8; 68];
        raw[FO76_LIGH_DATA_VALUE_OFFSET..FO76_LIGH_DATA_VALUE_OFFSET + 4]
            .copy_from_slice(&250_000_u32.to_le_bytes());
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(SmallVec::from_vec(raw)),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw DATA bytes");
        };
        assert_eq!(
            u32::from_le_bytes(
                bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            FO4_LIGH_MAX_SYNTHETIC_RADIUS
        );
        assert_eq!(
            f32::from_le_bytes(
                bytes[FO4_LIGH_DATA_SCALAR_OFFSET..FO4_LIGH_DATA_SCALAR_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            FO4_LIGH_DEFAULT_SCALAR
        );
        assert_eq!(
            u32::from_le_bytes(
                bytes[FO4_LIGH_DATA_VALUE_OFFSET..FO4_LIGH_DATA_VALUE_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            FO4_LIGH_DEFAULT_VALUE
        );
    }

    #[test]
    fn post_translate_sets_structured_light_radius_from_value() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("LIGH", &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (interner.intern("Value"), FieldValue::Uint(400)),
                (
                    interner.intern("Bytes19"),
                    FieldValue::Bytes(SmallVec::from_vec(vec![0; 8])),
                ),
            ]),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("expected structured DATA");
        };
        let radius = fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("Radius"))
            .map(|(_, value)| value);
        assert_eq!(radius, Some(&FieldValue::Uint(400)));
        assert!(
            fields
                .iter()
                .all(|(name, _)| interner.resolve(*name) != Some("Value")
                    && interner.resolve(*name) != Some("Bytes19"))
        );
        let scalar = fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("Scalar"))
            .map(|(_, value)| value);
        let exponent = fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("Exponent"))
            .map(|(_, value)| value);
        assert_eq!(scalar, Some(&FieldValue::Float(FO4_LIGH_DEFAULT_SCALAR)));
        assert_eq!(
            exponent,
            Some(&FieldValue::Float(FO4_LIGH_DEFAULT_EXPONENT))
        );
    }

    #[test]
    fn post_translate_clamps_missing_structured_light_radius_from_large_value() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("LIGH", &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![(interner.intern("Value"), FieldValue::Uint(250_000))]),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("expected structured DATA");
        };
        let radius = fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("Radius"))
            .map(|(_, value)| value);
        assert_eq!(
            radius,
            Some(&FieldValue::Uint(u64::from(FO4_LIGH_MAX_SYNTHETIC_RADIUS)))
        );
    }

    #[test]
    fn post_translate_caps_raw_cage_bulb_gobo_light_radius() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("LIGH", &interner);
        let mut raw = vec![0_u8; 68];
        raw[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
            .copy_from_slice(&1200_u32.to_le_bytes());
        raw[FO4_LIGH_DATA_NEAR_CLIP_OFFSET..FO4_LIGH_DATA_NEAR_CLIP_OFFSET + 4]
            .copy_from_slice(&1.0_f32.to_le_bytes());
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(SmallVec::from_vec(raw)),
        );
        push_field(
            &mut record,
            "NAM0",
            FieldValue::String(
                interner.intern("data\\Textures\\Effects\\Gobos\\CageBulbGobo01_d.DDS"),
            ),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw DATA bytes");
        };
        assert_eq!(
            u32::from_le_bytes(
                bytes[FO4_LIGH_DATA_RADIUS_OFFSET..FO4_LIGH_DATA_RADIUS_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            FO4_CAGE_BULB_GOBO_MAX_RADIUS
        );
        assert_eq!(
            f32::from_le_bytes(
                bytes[FO4_LIGH_DATA_NEAR_CLIP_OFFSET..FO4_LIGH_DATA_NEAR_CLIP_OFFSET + 4]
                    .try_into()
                    .unwrap()
            ),
            FO4_CAGE_BULB_GOBO_MIN_NEAR_CLIP
        );
    }

    #[test]
    fn post_translate_caps_structured_cage_bulb_gobo_light_radius() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("LIGH", &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Struct(vec![
                (interner.intern("Radius"), FieldValue::Uint(1200)),
                (interner.intern("NearClip"), FieldValue::Float(1.0)),
                (interner.intern("Value"), FieldValue::Uint(1200)),
            ]),
        );
        push_field(
            &mut record,
            "NAM0",
            FieldValue::String(interner.intern("Textures\\Effects\\Gobos\\CageBulbGobo01_d.DDS")),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("expected structured DATA");
        };
        let radius = fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("Radius"))
            .map(|(_, value)| value);
        let near_clip = fields
            .iter()
            .find(|(name, _)| interner.resolve(*name) == Some("NearClip"))
            .map(|(_, value)| value);
        assert_eq!(
            radius,
            Some(&FieldValue::Uint(u64::from(FO4_CAGE_BULB_GOBO_MAX_RADIUS)))
        );
        assert_eq!(
            near_clip,
            Some(&FieldValue::Float(FO4_CAGE_BULB_GOBO_MIN_NEAR_CLIP))
        );
    }

    #[test]
    fn post_translate_inserts_missing_light_fade_value_after_data() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("LIGH", &interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(SmallVec::from_vec(vec![0_u8; 64])),
        );

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["DATA", "FNAM"]);
        assert_eq!(
            record.fields[1].value,
            FieldValue::Float(FO4_LIGH_DEFAULT_FADE)
        );
    }

    #[test]
    fn post_translate_keeps_existing_light_fade_value() {
        let interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        let mut record = make_record("LIGH", &interner);
        push_field(&mut record, "FNAM", FieldValue::Float(0.25));

        hook.post_translate(&mut ctx, &mut record).unwrap();

        let fades: Vec<&FieldValue> = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "FNAM")
            .map(|field| &field.value)
            .collect();
        assert_eq!(fades, vec![&FieldValue::Float(0.25)]);
    }

    #[test]
    fn post_translate_drops_perk_vmad() {
        let interner = StringInterner::new();
        let mut record = make_record("PERK", &interner);
        push_field(&mut record, "EDID", FieldValue::None);
        push_field(
            &mut record,
            "VMAD",
            FieldValue::Bytes(SmallVec::from_vec(vec![1, 2, 3, 4])),
        );
        push_field(
            &mut record,
            "FULL",
            FieldValue::String(interner.intern("Perk")),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["EDID", "FULL"]);
    }

    #[test]
    fn post_translate_preserves_race_late_field_order() {
        let interner = StringInterner::new();
        let mut record = make_record("RACE", &interner);
        for sig in ["TTED", "MPPF", "MSM0", "BSMS", "MPPM", "TTGE", "MSM1"] {
            push_field(&mut record, sig, FieldValue::None);
        }

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect();
        assert_eq!(
            sigs,
            vec!["TTED", "MPPF", "MSM0", "BSMS", "MPPM", "TTGE", "MSM1"]
        );
    }

    #[test]
    fn pre_translate_strips_race_tints_without_dropping_conditions_or_morphs() {
        let interner = StringInterner::new();
        let mut race = make_record("RACE", &interner);
        push_field(&mut race, "EDID", FieldValue::None);
        push_field(&mut race, "ATKD", FieldValue::None);
        push_field(&mut race, "CTDA", FieldValue::None);
        push_field(&mut race, "CIS1", FieldValue::None);
        push_field(&mut race, "CIS2", FieldValue::None);
        push_field(&mut race, "HEAD", FieldValue::None);
        push_field(&mut race, "CTDA", FieldValue::None);
        push_field(&mut race, "CIS1", FieldValue::None);
        push_field(&mut race, "CIS2", FieldValue::None);
        for sig in [
            "TINL", "TTGP", "TETI", "TTEF", "CTDA", "CIS1", "CIS2", "TTET", "TTEB", "TTEC", "TTED",
            "TTGE",
        ] {
            push_field(&mut race, sig, FieldValue::None);
        }
        for sig in [
            "MPGN", "MPPC", "MPPI", "MPPN", "MPPM", "MPPT", "MPPF", "MPPK", "MPGS",
        ] {
            push_field(&mut race, sig, FieldValue::None);
        }

        let hook = Fo76Fo4Hook;
        hook.pre_translate(&mut make_ctx(&interner), &mut race)
            .unwrap();
        assert_eq!(
            race.fields
                .iter()
                .map(|entry| entry.sig.as_str())
                .collect::<Vec<_>>(),
            vec![
                "EDID", "ATKD", "CTDA", "CIS1", "CIS2", "HEAD", "CTDA", "CIS1", "CIS2", "MPGN",
                "MPPC", "MPPI", "MPPN", "MPPM", "MPPT", "MPPF", "MPPK", "MPGS",
            ]
        );

        let mut npc = make_record("NPC_", &interner);
        push_field(&mut npc, "QNAM", FieldValue::None);
        hook.pre_translate(&mut make_ctx(&interner), &mut npc)
            .unwrap();
        assert_eq!(npc.fields[0].sig.as_str(), "QNAM");
    }

    #[test]
    fn post_translate_masks_only_fo76_idlm_unknown_5_flag() {
        let interner = StringInterner::new();
        let mut record = make_record("IDLM", &interner);
        push_field(&mut record, "IDLF", FieldValue::Uint(0x3f));
        push_field(&mut record, "IDLF", FieldValue::Int(0x28));
        push_field(&mut record, "IDLF", raw_bytes(&[0x28]));
        push_field(&mut record, "IDLF", raw_bytes(&[0x28, 0xff]));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(record.fields[0].value, FieldValue::Uint(0x1f));
        assert_eq!(record.fields[1].value, FieldValue::Int(0x08));
        assert_eq!(record.fields[2].value, raw_bytes(&[0x08]));
        assert_eq!(record.fields[3].value, raw_bytes(&[0x28, 0xff]));
    }

    #[test]
    fn post_translate_does_not_mask_idlf_on_other_records() {
        let interner = StringInterner::new();
        let mut record = make_record("PACK", &interner);
        push_field(&mut record, "IDLF", FieldValue::Uint(0x28));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(record.fields[0].value, FieldValue::Uint(0x28));
    }

    #[test]
    fn post_translate_idlm_flag_mask_is_idempotent() {
        let interner = StringInterner::new();
        let mut record = make_record("IDLM", &interner);
        push_field(&mut record, "IDLF", FieldValue::Uint(0x28));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();
        let once = record.fields.clone();
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(record.fields, once);
        assert_eq!(record.fields[0].value, FieldValue::Uint(0x08));
    }

    #[test]
    fn post_translate_drops_raw_regn_rdot() {
        let mut interner = StringInterner::new();
        let mut record = make_record("REGN", &mut interner);
        push_field(&mut record, "RDAT", FieldValue::None);
        push_field(
            &mut record,
            "RDOT",
            FieldValue::Bytes(SmallVec::from_vec(vec![0_u8; 456])),
        );
        push_field(&mut record, "RDWT", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"RDAT"));
        assert!(!sigs.contains(&"RDOT"));
        assert!(sigs.contains(&"RDWT"));
    }

    #[test]
    fn post_translate_does_not_mark_named_border_region_when_flag_is_missing() {
        let interner = StringInterner::new();
        let mut record = make_record("REGN", &interner);
        record.eid = Some(interner.intern("BurningSpringsBorderRegion01"));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert!(!record.flags.contains(RecordFlags::BORDER_REGION));
    }

    #[test]
    fn post_translate_preserves_source_border_region_flag() {
        let interner = StringInterner::new();
        let mut record = make_record("REGN", &interner);
        record.eid = Some(interner.intern("BurningSpringsRegion"));
        record.flags.insert(RecordFlags::BORDER_REGION);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert!(record.flags.contains(RecordFlags::BORDER_REGION));
    }

    #[test]
    fn post_translate_does_not_use_rcbn_to_mark_region_border() {
        let interner = StringInterner::new();
        let mut record = make_record("REGN", &interner);
        record.eid = Some(interner.intern("ForestObjectRegion"));
        push_field(&mut record, "RCBN", raw_bytes(&[1]));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert!(!record.flags.contains(RecordFlags::BORDER_REGION));
    }

    #[test]
    fn post_translate_converts_structured_regn_rdot_to_fo4_rows() {
        let mut interner = StringInterner::new();
        let masters = vec!["SeventySix.esm".to_string()];
        let mut payload = vec![0_u8; 76];
        payload[12..16].copy_from_slice(&1.0f32.to_le_bytes());
        payload[44..48].copy_from_slice(&64.0f32.to_le_bytes());
        payload[48..52].copy_from_slice(&(-200000.0f32).to_le_bytes());
        payload[52..56].copy_from_slice(&200000.0f32.to_le_bytes());
        payload[64] = 10;
        payload[65] = 20;
        payload[66] = 30;
        payload[67] = 1;
        payload[68..72].copy_from_slice(&0x0000_0800u32.to_le_bytes());
        payload[72..74].copy_from_slice(&0xFFFFu16.to_le_bytes());

        let decoded =
            crate::fo76_rdot::decode_fo76_regn_rdot(&payload, &masters, "Source.esm", &interner)
                .expect("FO76 RDOT decodes");
        let mut record = make_record("REGN", &interner);
        push_field(&mut record, "RDAT", FieldValue::None);
        push_field(&mut record, "RDOT", decoded);
        push_field(&mut record, "RDWT", FieldValue::None);

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let rdot = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "RDOT")
            .expect("converted RDOT is preserved");
        let FieldValue::List(rows) = &rdot.value else {
            panic!("expected FO4 RDOT row list");
        };
        assert_eq!(rows.len(), 1);

        let schema = AuthoringSchema::for_game("fo4").expect("FO4 schema loads");
        let encoded =
            crate::target_write::encode_field_pub(rdot, schema.record_def("REGN"), &interner)
                .expect("converted RDOT encodes");
        assert_eq!(encoded.len(), 52);
    }

    #[test]
    fn post_translate_keeps_rdot_on_non_region_records() {
        let mut interner = StringInterner::new();
        let mut record = make_record("STAT", &mut interner);
        push_field(
            &mut record,
            "RDOT",
            FieldValue::Bytes(SmallVec::from_vec(vec![0_u8; 456])),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert!(sigs.contains(&"RDOT"));
    }

    #[test]
    fn post_translate_leaves_unprefixed_model_paths_unprefixed() {
        let mut interner = StringInterner::new();
        let mut record = make_record("STAT", &mut interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("Landscape\\Trees\\Tree.nif")),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(record.fields.len(), 1);
        let FieldValue::String(sym) = record.fields[0].value else {
            panic!("expected model path string");
        };
        assert_eq!(interner.resolve(sym), Some("Landscape\\Trees\\Tree.nif"));
    }

    #[test]
    fn post_translate_strips_source_prefixed_model_paths() {
        let mut interner = StringInterner::new();
        let mut record = make_record("STAT", &mut interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("fo76\\Landscape\\Plants\\MtnTopCreosote03.nif")),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::String(sym) = record.fields[0].value else {
            panic!("expected model path string");
        };
        assert_eq!(
            interner.resolve(sym),
            Some("Landscape\\Plants\\MtnTopCreosote03.nif")
        );
    }

    #[test]
    fn post_translate_strips_meshes_and_source_prefix_from_model_paths() {
        let mut interner = StringInterner::new();
        let mut record = make_record("STAT", &mut interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("Meshes\\fo76\\Landscape\\Trees\\Tree.nif")),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::String(sym) = record.fields[0].value else {
            panic!("expected model path string");
        };
        assert_eq!(interner.resolve(sym), Some("Landscape\\Trees\\Tree.nif"));
    }

    #[test]
    fn synthesize_records_returns_empty() {
        let mut interner = StringInterner::new();
        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        assert!(hook.synthesize_records(&mut ctx).is_empty());
    }

    /// Build a 72-byte FO76-style LIGH DATA blob: flags @ +12, near clip @ +24,
    /// flicker intensity amplitude @ +32.
    fn fo76_ligh_data(flags: u32, near_clip: f32, flicker_intensity_amp: f32) -> Vec<u8> {
        let mut bytes = vec![0_u8; 72];
        bytes[12..16].copy_from_slice(&flags.to_le_bytes());
        bytes[24..28].copy_from_slice(&near_clip.to_le_bytes());
        bytes[32..36].copy_from_slice(&flicker_intensity_amp.to_le_bytes());
        bytes
    }

    fn ligh_with_data(interner: &StringInterner, data: Vec<u8>, gobo: Option<&str>) -> Record {
        let mut record = make_record("LIGH", interner);
        push_field(
            &mut record,
            "DATA",
            FieldValue::Bytes(SmallVec::from_vec(data)),
        );
        if let Some(path) = gobo {
            push_field(
                &mut record,
                "NAM0",
                FieldValue::String(interner.intern(path)),
            );
        }
        record
    }

    fn ligh_data_bytes(record: &Record) -> Vec<u8> {
        record
            .fields
            .iter()
            .find(|e| e.sig.0 == *b"DATA")
            .and_then(|e| match &e.value {
                FieldValue::Bytes(b) => Some(b.to_vec()),
                _ => None,
            })
            .expect("DATA bytes")
    }

    #[test]
    fn light_normalize_clears_nonspecular_and_masks_fo76_bits() {
        let interner = StringInterner::new();
        // FO76 barrel flags 0x8009 (unknown0 + flicker + non_specular) plus an
        // FO76-only high bit (0x800000) that FO4 does not define.
        let mut record = ligh_with_data(&interner, fo76_ligh_data(0x0080_8009, 1.0, 10.0), None);
        Fo76Fo4Hook::normalize_light_data_for_fo4(&interner, &mut record);
        let data = ligh_data_bytes(&record);
        let flags = u32::from_le_bytes(data[12..16].try_into().unwrap());
        assert_eq!(
            flags, 0x0000_0009,
            "non_specular and FO76-only bit dropped, unknown0 + flicker kept"
        );
    }

    #[test]
    fn light_normalize_floors_small_near_clip() {
        let interner = StringInterner::new();
        let mut record = ligh_with_data(&interner, fo76_ligh_data(0x0009, 1.0, 0.4), None);
        Fo76Fo4Hook::normalize_light_data_for_fo4(&interner, &mut record);
        let data = ligh_data_bytes(&record);
        let near = f32::from_le_bytes(data[24..28].try_into().unwrap());
        assert_eq!(near, FO4_LIGH_MIN_NEAR_CLIP);
    }

    #[test]
    fn light_normalize_preserves_large_near_clip() {
        let interner = StringInterner::new();
        let mut record = ligh_with_data(&interner, fo76_ligh_data(0x0009, 64.0, 0.4), None);
        Fo76Fo4Hook::normalize_light_data_for_fo4(&interner, &mut record);
        let data = ligh_data_bytes(&record);
        let near = f32::from_le_bytes(data[24..28].try_into().unwrap());
        assert_eq!(near, 64.0);
    }

    #[test]
    fn light_normalize_clamps_flicker_gobo_tighter_and_truncates() {
        let interner = StringInterner::new();
        let gobo = Some("Data\\textures\\effects\\gobos\\worklightgobo_d.dds");
        let mut record = ligh_with_data(&interner, fo76_ligh_data(0x8009, 1.0, 10.0), gobo);
        Fo76Fo4Hook::normalize_light_data_for_fo4(&interner, &mut record);
        let data = ligh_data_bytes(&record);
        let amp = f32::from_le_bytes(data[32..36].try_into().unwrap());
        assert_eq!(amp, FO4_LIGH_MAX_FLICKER_INTENSITY_AMP_GOBO);
        assert_eq!(
            data.len(),
            FO4_LIGH_DATA_LEN,
            "truncated to FO4 DATA length"
        );
    }

    #[test]
    fn light_normalize_clamps_flicker_nongobo_to_ceiling() {
        let interner = StringInterner::new();
        let mut record = ligh_with_data(&interner, fo76_ligh_data(0x8009, 1.0, 30000.0), None);
        Fo76Fo4Hook::normalize_light_data_for_fo4(&interner, &mut record);
        let data = ligh_data_bytes(&record);
        let amp = f32::from_le_bytes(data[32..36].try_into().unwrap());
        assert_eq!(amp, FO4_LIGH_MAX_FLICKER_INTENSITY_AMP);
    }

    #[test]
    fn light_normalize_leaves_in_range_flicker_untouched() {
        let interner = StringInterner::new();
        let gobo = Some("Data\\textures\\effects\\gobos\\worklightgobo_d.dds");
        let mut record = ligh_with_data(&interner, fo76_ligh_data(0x8009, 32.0, 0.45), gobo);
        Fo76Fo4Hook::normalize_light_data_for_fo4(&interner, &mut record);
        let data = ligh_data_bytes(&record);
        let amp = f32::from_le_bytes(data[32..36].try_into().unwrap());
        assert_eq!(
            amp, 0.45,
            "value already inside FO4's gobo range is preserved"
        );
    }

    fn term_snam_values(record: &Record) -> Vec<&FieldValue> {
        record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"SNAM")
            .map(|entry| &entry.value)
            .collect()
    }

    #[test]
    fn term_looping_sound_snam_is_stripped() {
        let interner = StringInterner::new();
        let mut record = make_record("TERM", &interner);
        push_field(
            &mut record,
            "SNAM",
            FieldValue::FormKey(FormKey::parse("800000@SeventySix.esm", &interner).unwrap()),
        );
        push_field(&mut record, "SNAM", FieldValue::None);
        push_field(
            &mut record,
            "SNAM",
            FieldValue::Bytes(SmallVec::from_vec(vec![0_u8; 4])),
        );

        Fo76Fo4Hook::strip_term_looping_sound_snam(&mut record);

        assert!(term_snam_values(&record).is_empty());
    }

    #[test]
    fn term_marker_parameter_snam_rows_are_kept() {
        let interner = StringInterner::new();
        let mut record = make_record("TERM", &interner);
        push_field(
            &mut record,
            "SNAM",
            FieldValue::Bytes(SmallVec::from_vec(vec![
                0_u8;
                FURNITURE_MARKER_PARAMETERS_ROW_LEN
                    * 2
            ])),
        );
        push_field(
            &mut record,
            "SNAM",
            FieldValue::List(vec![FieldValue::Struct(Vec::new())]),
        );

        Fo76Fo4Hook::strip_term_looping_sound_snam(&mut record);

        assert_eq!(term_snam_values(&record).len(), 2);
    }

    #[test]
    fn furn_snam_is_not_touched_by_term_strip() {
        let interner = StringInterner::new();
        let mut record = make_record("FURN", &interner);
        push_field(
            &mut record,
            "SNAM",
            FieldValue::FormKey(FormKey::parse("000123@SeventySix.esm", &interner).unwrap()),
        );

        Fo76Fo4Hook::strip_term_looping_sound_snam(&mut record);

        assert_eq!(term_snam_values(&record).len(), 1);
    }
}
