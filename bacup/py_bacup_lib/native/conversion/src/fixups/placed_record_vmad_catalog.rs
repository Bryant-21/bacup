use super::placed_record_vmad::{
    CellSection, PlacedRecordVmadPolicy, RecordRequirement, ScriptAdapter,
};

const SOURCE_PLUGIN: &str = "SeventySix.esm";

pub(crate) const FO76_TO_FO4_POLICIES: &[PlacedRecordVmadPolicy] = &[
    PlacedRecordVmadPolicy {
        label: "w05_mq_001p_alias15_whole_space_trigger",
        source_plugin: SOURCE_PLUGIN,
        placed: RecordRequirement::new("REFR", 0x0041_A392),
        parent_cell: RecordRequirement::new("CELL", 0x0040_41F2),
        section: CellSection::Temporary,
        base: RecordRequirement::new("ACTI", 0x0018_DC72),
        reference_type: Some(RecordRequirement::new("LCRT", 0x0041_A350)),
        adapter: ScriptAdapter::StoryEventOnTriggerEnter {
            target_quest: RecordRequirement::new("QUST", 0x0040_5E14),
            story_event_keyword: RecordRequirement::new("KYWD", 0x0040_5EC6),
            stage_to_set: 400,
        },
        evidence: "QUST 405E14 alias 15 WholeSpaceTrigger uses RefType 41A350 and DefaultAliasOnTriggerEnter stage 400",
    },
    PlacedRecordVmadPolicy {
        label: "w05_mq_001p_lacey_activation_compatibility_policy",
        source_plugin: SOURCE_PLUGIN,
        placed: RecordRequirement::new("ACHR", 0x0040_5EC1),
        parent_cell: RecordRequirement::new("CELL", 0x0026_3D53),
        section: CellSection::Temporary,
        base: RecordRequirement::new("NPC_", 0x0040_5EB0),
        reference_type: None,
        adapter: ScriptAdapter::StoryEventOnActivateStartScene {
            target_quest: RecordRequirement::new("QUST", 0x0040_5E15),
            story_event_keyword: RecordRequirement::new("KYWD", 0x0040_5EC7),
            scene_to_start: RecordRequirement::new("SCEN", 0x0040_5ED2),
        },
        evidence: "compatibility policy for player activation after the source multiplayer interaction bridge was removed",
    },
    PlacedRecordVmadPolicy {
        label: "w05_mq_001p_alias9_interior_scene_start_trigger",
        source_plugin: SOURCE_PLUGIN,
        placed: RecordRequirement::new("REFR", 0x0040_BD12),
        parent_cell: RecordRequirement::new("CELL", 0x0040_41F2),
        section: CellSection::Temporary,
        base: RecordRequirement::new("ACTI", 0x0018_DC72),
        reference_type: Some(RecordRequirement::new("LCRT", 0x0040_BD14)),
        adapter: ScriptAdapter::StoryEventOnTriggerEnter {
            target_quest: RecordRequirement::new("QUST", 0x0040_5E14),
            story_event_keyword: RecordRequirement::new("KYWD", 0x0040_5EC6),
            stage_to_set: 500,
        },
        evidence: "QUST 405E14 alias 9 IntSceneStartTrigger uses RefType 40BD14 and stage 500 starts scene 40BD20",
    },
    PlacedRecordVmadPolicy {
        label: "vault79_mine_collapse_trigger_marker_binding",
        source_plugin: SOURCE_PLUGIN,
        placed: RecordRequirement::new("REFR", 0x0058_B424),
        parent_cell: RecordRequirement::new("CELL", 0x0040_1116),
        section: CellSection::Temporary,
        base: RecordRequirement::new("ACTI", 0x0018_DC72),
        reference_type: None,
        adapter: ScriptAdapter::BindExistingPlacedObject {
            script_name: "Vault79MineCollapseScript",
            property_name: "myCollapseMarker",
            target: RecordRequirement::new("REFR", 0x0055_862E),
            target_parent_cell: RecordRequirement::new("CELL", 0x0040_1116),
            target_section: CellSection::Persistent,
            target_base: RecordRequirement::new("MSTT", 0x0000_1D5E),
        },
        evidence: "source 58B424 omits mandatory myCollapseMarker; same-cell Vault79 tunnel controller 53AD43 binds that exact property to persistent CollapsingMine 55862E",
    },
];
