use super::*;
use esp_authoring_core::plugin_runtime::effective_subrecords_for_record;
use std::sync::{Arc, OnceLock, RwLock};

/// Highest FO4 condition function id currently represented in the FO4 schema.
///
/// FO76 CTDA/CTDT subrecords can carry FO76-only function ids. The FO4 CK
/// indexes its condition-function table with those ids while loading and can
/// crash before it has a chance to report a warning.
pub(super) const FO4_MAX_KNOWN_CONDITION_FUNCTION_ID: u16 = 817;
pub(super) const FO76_IS_IN_INTERIOR_ACOUSTIC_SPACE_CONDITION_FUNCTION_ID: u16 = 276;
pub(super) const FO4_IS_IN_INTERIOR_CONDITION_FUNCTION_ID: u16 = 300;
// MUST conditions run against FO4's combat target; other record types cannot
// safely approximate FO76's strongest-enemy context.
pub(super) const FO76_GET_STRONGEST_ENEMY_HAS_KEYWORD_CONDITION_FUNCTION_ID: u16 = 692;
pub(super) const FO4_GET_COMBAT_TARGET_HAS_KEYWORD_CONDITION_FUNCTION_ID: u16 = 707;
/// FO76 `GetIsCurrentLocationExact` (844, > FO4's 817 max). It takes an LCTN in
/// Parameter #1; FO4 `GetInCurrentLocation` (359) is the closest compatible gate.
pub(super) const FO76_GET_IS_CURRENT_LOCATION_EXACT_CONDITION_FUNCTION_ID: u16 = 844;
pub(super) const FO4_GET_IN_CURRENT_LOCATION_CONDITION_FUNCTION_ID: u16 = 359;
/// FO76 `EditorLocationHasKeyword` (579) and FO4 `LocationHasKeyword` (562)
/// both take a KYWD in Parameter #1 and test the owning reference's location.
pub(super) const FO76_EDITOR_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID: u16 = 579;
pub(super) const FO4_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID: u16 = 562;
/// FO76-only `IsQuestActive` (876, > FO4's 817 max). It takes a QUST in
/// Parameter #1 and is compared `== 1`, so it maps value-identically onto FO4
/// `GetQuestRunning` (56). Remapped before the incompatibility drop so the
/// gating condition survives instead of being stripped (which would leave the
/// owning record — e.g. a loading screen — unconditionally eligible).
pub(super) const FO76_IS_QUEST_ACTIVE_CONDITION_FUNCTION_ID: u16 = 876;
/// FO76 `GetQuestRunningUnique` (856) takes a QUST in Parameter #1 and returns
/// the same running-state predicate as FO4 `GetQuestRunning` (56).
pub(super) const FO76_GET_QUEST_RUNNING_UNIQUE_CONDITION_FUNCTION_ID: u16 = 856;
pub(super) const FO4_GET_QUEST_RUNNING_CONDITION_FUNCTION_ID: u16 = 56;
pub(super) const FO76_GET_NUM_TIMES_COMPLETED_QUEST_CONDITION_FUNCTION_ID: u16 = 857;
pub(super) const FO4_GET_QUEST_COMPLETED_CONDITION_FUNCTION_ID: u16 = 543;
/// Completion counts probed when deciding whether a `GetNumTimesCompletedQuest`
/// predicate can be carried onto the boolean `GetQuestCompleted`.
const FO76_COMPLETION_COUNT_PROBES: [f32; 6] = [1.0, 2.0, 3.0, 5.0, 10.0, 1.0e9];
pub(super) const FO76_GET_EVENT_DATA_CONDITION_FUNCTION_ID: u16 = 576;
const FO76_EVENT_DATA_KEYWORD_1_SELECTOR: u32 = 0x314B_0000;
const FO76_EVENT_DATA_LEVEL_SELECTOR: u32 = 0x3156_0002;
pub(super) const FO4_INCREASE_LEVEL_STORY_MANAGER_ROOT: u32 = 0x0016_556F;
pub(super) const FO76_HAS_KEYWORD_CONDITION_FUNCTION_ID: u16 = 560;
const FO76_EVENT_DATA_REFERENCE_3_SELECTOR: u32 = 0x0000_3352;
const CTDA_RUN_ON_SUBJECT: u32 = 0;
const CTDA_RUN_ON_TARGET: u32 = 1;
const CTDA_RUN_ON_REFERENCE: u32 = 2;
const CTDA_RUN_ON_EVENT_DATA: u32 = 7;
const FO4_GET_IS_SEX_CONDITION_FUNCTION_ID: u16 = 70;
const WAYWARD_PLAYER_GENDER_INFO_FORM_IDS: [u32; 4] = [0x56_A169, 0x56_A16A, 0x56_A178, 0x56_A179];
/// FO76 `GetIsPlayer` has no FO4 condition-function slot. FO4 expresses the
/// same predicate as `GetIsID` with Actor: Player in Parameter #1.
pub(super) const FO76_GET_IS_PLAYER_CONDITION_FUNCTION_ID: u16 = 828;
pub(super) const FO4_GET_IS_ID_CONDITION_FUNCTION_ID: u16 = 72;
pub(super) const FO4_PLAYER_ACTOR_FORM_ID: u32 = 0x0000_0007;
/// Every FO76-only condition-function id that `normalize_fo76_raw_condition_functions`
/// rewrites to an FO4 equivalent. Consumed by the
/// `drop_untranslatable_loadscreen_records` fixup so it does NOT treat a
/// remapped function as untranslatable. Keep in sync with the remaps below.
pub(crate) const FO76_REMAPPED_CONDITION_FUNCTION_IDS: &[u16] = &[
    FO76_IS_IN_INTERIOR_ACOUSTIC_SPACE_CONDITION_FUNCTION_ID,
    FO76_EDITOR_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID,
    FO76_GET_IS_PLAYER_CONDITION_FUNCTION_ID,
    FO76_GET_IS_CURRENT_LOCATION_EXACT_CONDITION_FUNCTION_ID,
    FO76_GET_QUEST_RUNNING_UNIQUE_CONDITION_FUNCTION_ID,
    FO76_IS_QUEST_ACTIVE_CONDITION_FUNCTION_ID,
];
/// FO76-only condition ids below FO4's max id. The FO4 CK still treats these as
/// blank condition functions while loading, so the max-id guard is not enough.
/// 596 is an FO76-only function carried with a `$73808CE`-style Parameter #1 on
/// BS01 Brotherhood dialogue INFOs; xEdit renders it as `<Unknown:param>` and
/// the FO4 CK indexes its blank slot while loading (4 INFO records).
pub(super) const FO76_ONLY_CONDITION_FUNCTION_IDS_UNDER_FO4_MAX: &[u16] =
    &[2, 3, 105, 371, 579, 596, 692, 730, 737];
// CK rejects exterior CELL parameters for this COBJ condition and can crash while loading.
pub(super) const FO4_COBJ_EXTERIOR_CELL_REJECTED_CONDITION_FUNCTION_ID: u16 = 310;
// FO76 Function 67 carries source-side function-info/base-object values that
// FO4 CK tries to resolve from its own Function Info table while loading.
pub(super) const FO76_FUNCTION_INFO_CONDITION_FUNCTION_ID: u16 = 67;
// FO4 condition functions whose Parameter #1 is a QUST FormID
// (wbDefinitionsFO4.pas, Paramtype1: ptQuest). A FO76 QUST referenced here may
// be dropped/unconverted, leaving Parameter #1 = NULL. xEdit then reports the
// CTDA as "Parameter #1 -> Found NULL, expected QUST". The condition can't be
// retargeted (there is no surviving quest), so the whole CTDA is dropped.
pub(super) const FO4_QUEST_PARAMETER_1_CONDITION_FUNCTION_IDS: &[u16] =
    &[56, 58, 59, 543, 629, 664];
pub(super) const FO4_GET_STAGE_CONDITION_FUNCTION_ID: u16 = 58;
// CTDA "Run On" value (bytes [20..24]) for "Quest Alias": the condition resolves
// through an alias index against the owning quest.
pub(super) const CTDA_RUN_ON_QUEST_ALIAS: u32 = 5;
// Record types whose CTDA carries an owning quest context (xEdit resolves it
// from the record container or QNAM/PNAM-style owner field).
pub(super) const QUEST_CONTEXT_CONDITION_RECORD_SIGS: &[[u8; 4]] =
    &[*b"QUST", *b"SCEN", *b"PACK", *b"INFO", *b"DIAL"];
// GetIsAliasRef / alias-index parameter. It is valid only when xEdit can resolve
// an owning quest context and that quest has the referenced alias id.
pub(super) const FO4_QUEST_ALIAS_PARAMETER_1_CONDITION_FUNCTION_IDS: &[u16] = &[566];
// FO76 nuke-zone check (849, > FO4's 817 max — always dropped as untranslatable).
// Gates leveled-list entries to nuke-irradiated zones (radiation suits, glowing
// variants) that never occur in FO4.
pub(super) const FO76_NUKE_ZONE_CONDITION_FUNCTION_ID: u16 = 849;
pub(super) const CTDA_COMPARISON_GLOBAL_FLAG: u8 = 0x04;

/// True when a CTDA function id has no FO4 equivalent: it exceeds FO4's max
/// known condition-function id (817), or it is a FO76-only id that falls under
/// that max but is still a blank slot in FO4. Keeping such a condition makes the
/// FO4 CK index a non-existent function-table entry and crash while loading.
pub(crate) fn is_fo4_incompatible_condition_function_id(function_id: u16) -> bool {
    function_id > FO4_MAX_KNOWN_CONDITION_FUNCTION_ID
        || FO76_ONLY_CONDITION_FUNCTION_IDS_UNDER_FO4_MAX.contains(&function_id)
}

/// Rewrite a condition in place into FO4 `GetIsID(Player) == 1`, which is false for
/// every actor that is not the player, and a package never runs on the player.
/// Returns false when the row is too short to hold a complete FO4 condition; the
/// caller then drops it.
///
/// Removing a gate makes its record MORE permissive, and a package with no
/// surviving conditions doesn't go quiet: it runs on every actor that carries it
/// and outranks everything below it in the stack. FO76 `AttackScentAttractorMeat`
/// (33D253) is gated on function 854 against a scent-attractor hazard; stripped,
/// it runs unconditionally on converted Mole Miners and pins them to its
/// `PreferredSpeed: Jog`, so they never sprint.
///
/// The OR flag is kept so the row holds its place in the record's OR groups: an
/// untranslatable ANDed gate closes the whole package, an OR'd one only retires
/// its own branch. Every other flag bit is cleared, mainly the comparison
/// operator in bits 5-7: inheriting `Not equal to` would make the closed gate
/// always true.
pub(crate) fn close_condition_gate(bytes: &mut [u8]) -> bool {
    if bytes.len() < 24 {
        return false;
    }
    bytes[0] &= CTDA_OR_FLAG;
    bytes[1..4].fill(0);
    bytes[4..8].copy_from_slice(&1.0f32.to_le_bytes());
    Fo76Fo4Hook::set_raw_condition_function_id(bytes, FO4_GET_IS_ID_CONDITION_FUNCTION_ID);
    bytes[10..12].fill(0);
    Fo76Fo4Hook::set_raw_condition_parameter_1(bytes, FO4_PLAYER_ACTOR_FORM_ID);
    Fo76Fo4Hook::set_raw_condition_parameter_2(bytes, 0);
    Fo76Fo4Hook::set_raw_condition_run_on(bytes, CTDA_RUN_ON_SUBJECT);
    Fo76Fo4Hook::set_raw_condition_reference(bytes, 0);
    Fo76Fo4Hook::set_raw_condition_parameter_3(bytes, 0);
    true
}

impl Fo76Fo4Hook {
    pub(super) fn drop_fo4_incompatible_conditions(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        Self::normalize_fo76_raw_condition_functions(record);
        let record_sig = record.sig.0;
        let record_local = record.form_key.local;
        let trace_drops = crate::drop_trace::enabled();
        // When a CTDA is dropped, its trailing CIS1/CIS2 parameter strings must
        // be dropped with it: they immediately follow their owning condition and
        // FO4 rejects a CIS1/CIS2 that is not preceded by a CTDA (CK/xEdit report
        // it as an out-of-order subrecord, e.g. orphaned `BTXT CIS2` rows in TERM
        // body/menu condition groups).
        // A package with no surviving conditions runs unconditionally rather than
        // going quiet — see `close_condition_gate`.
        let close_gates_instead_of_dropping = record_sig == *b"PACK";
        let mut dropping_condition_strings = false;
        record.fields.retain_mut(|entry| match &entry.sig.0 {
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
                            let deferred_context_get_stage = QUEST_CONTEXT_CONDITION_RECORD_SIGS
                                .contains(&record_sig)
                                && function_id == FO4_GET_STAGE_CONDITION_FUNCTION_ID
                                && Self::is_source_shaped_context_get_stage(&entry.value);
                            if !deferred_context_get_stage {
                                return true;
                            }
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
                if drop && trace_drops {
                    // An ANDed condition that vanishes makes its owning record
                    // strictly MORE permissive (a dialogue INFO plays when it
                    // should not), so the OR flag is part of the record.
                    let anded = !matches!(&entry.value, FieldValue::Bytes(bytes)
                        if bytes.first().is_some_and(|flags| flags & 0x01 != 0));
                    crate::drop_trace::trace(
                        "fo76_fo4.conditions",
                        std::str::from_utf8(&record_sig).unwrap_or("????"),
                        record_local,
                        std::str::from_utf8(&entry.sig.0).unwrap_or("CTDA"),
                        &format!(
                            "fo4-incompatible condition dropped; function={} parameter_1={:08X} \
                             run_on={} anded={anded}",
                            Self::condition_function_id(interner, &entry.value).unwrap_or(0),
                            Self::condition_parameter_1(interner, &entry.value).unwrap_or(0),
                            Self::condition_run_on(interner, &entry.value).unwrap_or(0),
                        ),
                    );
                }
                if drop
                    && close_gates_instead_of_dropping
                    && Self::close_condition_gate(&mut entry.value)
                {
                    // The rewritten gate no longer runs the function its CIS1/CIS2
                    // parameter strings named, so they go with the original.
                    dropping_condition_strings = true;
                    return true;
                }
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

    fn close_condition_gate(value: &mut FieldValue) -> bool {
        match value {
            FieldValue::Bytes(bytes) => close_condition_gate(bytes),
            _ => false,
        }
    }

    fn is_source_shaped_context_get_stage(value: &FieldValue) -> bool {
        let FieldValue::Bytes(bytes) = value else {
            return false;
        };
        bytes.len() == 32
            && Self::raw_condition_function_id(bytes) == Some(FO4_GET_STAGE_CONDITION_FUNCTION_ID)
            && Self::raw_condition_parameter_1(bytes) == Some(0)
            && Self::raw_condition_parameter_2(bytes) == Some(0)
            && Self::raw_condition_run_on(bytes) == Some(CTDA_RUN_ON_SUBJECT)
            && Self::raw_condition_reference(bytes) == Some(0)
            && Self::raw_condition_parameter_3(bytes) == Some(u32::MAX)
            && Self::raw_condition_operator(bytes).is_some_and(|operator| operator <= 5)
            && Self::raw_condition_comparison_value(bytes).is_some_and(|comparison| {
                comparison.is_finite()
                    && comparison >= 0.0
                    && comparison <= u16::MAX as f32
                    && comparison.fract() == 0.0
            })
    }

    pub(crate) fn story_manager_start_keyword(value: &FieldValue) -> Option<u32> {
        let FieldValue::Bytes(bytes) = value else {
            return None;
        };
        (bytes.len() >= 32
            && bytes[0] == 0
            && Self::raw_condition_function_id(bytes)
                == Some(FO76_GET_EVENT_DATA_CONDITION_FUNCTION_ID)
            && Self::raw_condition_parameter_1(bytes) == Some(FO76_EVENT_DATA_KEYWORD_1_SELECTOR)
            && Self::raw_condition_parameter_2(bytes).is_some_and(|keyword| keyword != 0)
            && Self::raw_condition_run_on(bytes) == Some(CTDA_RUN_ON_SUBJECT)
            && Self::raw_condition_reference(bytes) == Some(0)
            && Self::raw_condition_parameter_3(bytes) == Some(u32::MAX)
            && Self::raw_condition_comparison_value(bytes) == Some(1.0))
        .then(|| Self::raw_condition_parameter_2(bytes).unwrap())
    }

    pub(crate) fn story_manager_active_keyword(value: &FieldValue) -> Option<u32> {
        let FieldValue::Bytes(bytes) = value else {
            return None;
        };
        (bytes.len() >= 32
            && bytes[0] == 0
            && Self::raw_condition_function_id(bytes)
                == Some(FO76_HAS_KEYWORD_CONDITION_FUNCTION_ID)
            && Self::raw_condition_parameter_1(bytes).is_some_and(|keyword| keyword != 0)
            && Self::raw_condition_parameter_2(bytes) == Some(0)
            && Self::raw_condition_run_on(bytes) == Some(CTDA_RUN_ON_EVENT_DATA)
            && Self::raw_condition_reference(bytes) == Some(0)
            && Self::raw_condition_parameter_3(bytes) == Some(FO76_EVENT_DATA_REFERENCE_3_SELECTOR)
            && Self::raw_condition_comparison_value(bytes) == Some(0.0))
        .then(|| Self::raw_condition_parameter_1(bytes).unwrap())
    }

    pub(crate) fn story_manager_completion_quest(value: &FieldValue) -> Option<u32> {
        let FieldValue::Bytes(bytes) = value else {
            return None;
        };
        (bytes.len() >= 32
            && matches!(bytes[0], 0x00 | 0xA0)
            && Self::raw_condition_function_id(bytes)
                == Some(FO76_GET_NUM_TIMES_COMPLETED_QUEST_CONDITION_FUNCTION_ID)
            && Self::raw_condition_parameter_1(bytes).is_some_and(|quest| quest != 0)
            && Self::raw_condition_parameter_2(bytes) == Some(0)
            && Self::raw_condition_run_on(bytes) == Some(CTDA_RUN_ON_EVENT_DATA)
            && Self::raw_condition_reference(bytes) == Some(0)
            && Self::raw_condition_parameter_3(bytes) == Some(FO76_EVENT_DATA_REFERENCE_3_SELECTOR)
            && matches!(Self::raw_condition_operator(bytes), Some(0 | 5))
            && Self::raw_condition_comparison_value(bytes) == Some(0.0))
        .then(|| Self::raw_condition_parameter_1(bytes).unwrap())
    }

    /// NOT called by the conversion pipeline any more — `normalize_story_manager_
    /// quest_start_conditions` drops these gates instead of lowering them, because
    /// the lowered Subject-scoped form never matched at runtime and silently
    /// suppressed quest starts. Retained (with its tests) so the lowering can be
    /// restored if a correct FO4 encoding for the event-data gates is found.
    pub(crate) fn lower_story_manager_active_keyword_condition(
        value: &mut FieldValue,
        quest: u32,
    ) -> bool {
        if Self::story_manager_active_keyword(value).is_none() {
            return false;
        }
        let FieldValue::Bytes(bytes) = value else {
            return false;
        };
        Self::set_raw_condition_function_id(bytes, FO4_GET_QUEST_RUNNING_CONDITION_FUNCTION_ID);
        Self::set_raw_condition_parameter_1(bytes, quest);
        Self::normalize_story_manager_quest_condition(bytes);
        true
    }

    pub(crate) fn lower_story_manager_completion_condition(value: &mut FieldValue) -> bool {
        if Self::story_manager_completion_quest(value).is_none() {
            return false;
        }
        let FieldValue::Bytes(bytes) = value else {
            return false;
        };
        Self::set_raw_condition_function_id(bytes, FO4_GET_QUEST_COMPLETED_CONDITION_FUNCTION_ID);
        Self::normalize_story_manager_quest_condition(bytes);
        true
    }

    /// The FO4 `GetQuestCompleted` comparison value that carries a FO76
    /// `GetNumTimesCompletedQuest <operator> <comparison>` predicate, plus
    /// whether the carry is exact. `None` when no sound boolean exists.
    ///
    /// FO76 returns how many times the quest was completed; FO4 returns 0/1, so
    /// only the `0` vs `>= 1` split survives. Two cases translate:
    ///
    /// * the predicate holds exactly on `0` (`count < 1`, `== 0`, `<= 0`) →
    ///   `GetQuestCompleted == 0`;
    /// * the predicate is false at `0` (`>= 1`, `> 0`, and also `== 1`, `>= 2`)
    ///   → `GetQuestCompleted == 1`. Exact on a non-repeatable quest; on a
    ///   repeatable one it **widens** to counts the source excluded. That is
    ///   still tighter than dropping an ANDed condition (leaving the record
    ///   unconditionally eligible), and it never turns a source-true case
    ///   false, so it never suppresses a line the source allowed.
    ///
    /// Everything else is refused: `count <= 1` / `count < 3` on a repeatable
    /// daily hold on both `0` and some non-zero counts, so the only superset is
    /// the tautology, which is no better than dropping. Tautologies
    /// (`count >= 0`) and contradictions (`count < 0`) are refused too.
    pub(crate) fn quest_completed_comparison_for_count_predicate(
        operator: u8,
        comparison: f32,
    ) -> Option<(f32, bool)> {
        if operator > 5 || !comparison.is_finite() {
            return None;
        }
        let holds_at_zero = Self::numeric_condition_matches(0.0, operator, comparison);
        let positive_matches = FO76_COMPLETION_COUNT_PROBES
            .iter()
            .filter(|&&count| Self::numeric_condition_matches(count, operator, comparison))
            .count();
        match (holds_at_zero, positive_matches) {
            (true, 0) => Some((0.0, true)),
            (false, 0) => None,
            (false, matched) => Some((1.0, matched == FO76_COMPLETION_COUNT_PROBES.len())),
            (true, _) => None,
        }
    }

    /// Lower a plain (non-Story-Manager) FO76 `GetNumTimesCompletedQuest` CTDA
    /// onto FO4 `GetQuestCompleted`. `None` means the shape is not a lowerable
    /// 857 condition and the caller must drop *and report* it; `Some(exact)`
    /// reports whether the lowering was exact or a widening.
    pub(crate) fn lower_quest_completion_count_condition(value: &mut FieldValue) -> Option<bool> {
        if Self::story_manager_completion_quest(value).is_some()
            && matches!(value, FieldValue::Bytes(bytes)
                if Self::raw_condition_operator(bytes) == Some(5))
        {
            // Owned by the Story Manager phase, which still matches on 857.
            return None;
        }
        let FieldValue::Bytes(bytes) = value else {
            return None;
        };
        if bytes.len() < 32
            || Self::raw_condition_function_id(bytes)
                != Some(FO76_GET_NUM_TIMES_COMPLETED_QUEST_CONDITION_FUNCTION_ID)
            || Self::raw_condition_parameter_1(bytes).is_none_or(|quest| quest == 0)
        {
            return None;
        }
        let operator = Self::raw_condition_operator(bytes)?;
        // A GLOB-backed comparison value has no compile-time count to map.
        let comparison = Self::raw_condition_comparison_value(bytes)?;
        let (target, exact) =
            Self::quest_completed_comparison_for_count_predicate(operator, comparison)?;
        Self::set_raw_condition_function_id(bytes, FO4_GET_QUEST_COMPLETED_CONDITION_FUNCTION_ID);
        // Keep the CTDA flags (Or / Use Aliases / Swap Subject) and force the
        // operator to Equal against the boolean result.
        bytes[0] &= 0x1F;
        bytes[4..8].copy_from_slice(&target.to_le_bytes());
        // FO4 `GetQuestCompleted` is a global quest predicate: the source runOn
        // (Target / Quest Alias / 15) and its Parameter #2/#3 payload carry no
        // meaning for it, and a Quest Alias runOn would face alias-index
        // validation it cannot satisfy.
        Self::normalize_story_manager_quest_condition(bytes);
        Some(exact)
    }

    /// Lower every lowerable 857 condition on a record and trace the rest, which
    /// `drop_fo4_incompatible_conditions` then strips.
    pub(super) fn normalize_fo76_quest_completion_conditions(record: &mut Record) {
        let record_sig = record.sig.0;
        let local = record.form_key.local;
        for entry in &mut record.fields {
            if !matches!(&entry.sig.0, b"CTDA" | b"CTDT") {
                continue;
            }
            let is_completion_count = matches!(&entry.value, FieldValue::Bytes(bytes)
                if Self::raw_condition_function_id(bytes)
                    == Some(FO76_GET_NUM_TIMES_COMPLETED_QUEST_CONDITION_FUNCTION_ID));
            if !is_completion_count {
                continue;
            }
            let shape = match (crate::drop_trace::enabled(), &entry.value) {
                (true, FieldValue::Bytes(bytes)) => format!(
                    "operator={:?} comparison={:?} quest={:08X}",
                    Self::raw_condition_operator(bytes),
                    Self::raw_condition_comparison_value(bytes),
                    Self::raw_condition_parameter_1(bytes).unwrap_or(0),
                ),
                _ => String::new(),
            };
            let outcome = Self::lower_quest_completion_count_condition(&mut entry.value);
            if !crate::drop_trace::enabled() || matches!(outcome, Some(true)) {
                continue;
            }
            let reason = match outcome {
                Some(_) => format!(
                    "GetNumTimesCompletedQuest(857) WIDENED onto GetQuestCompleted(543); {shape}"
                ),
                None => format!(
                    "GetNumTimesCompletedQuest(857) not expressible as GetQuestCompleted(543); \
                     condition will be dropped; {shape}"
                ),
            };
            crate::drop_trace::trace(
                "fo76_fo4.conditions",
                std::str::from_utf8(&record_sig).unwrap_or("????"),
                local,
                "CTDA",
                &reason,
            );
        }
    }

    fn normalize_story_manager_quest_condition(bytes: &mut [u8]) {
        Self::set_raw_condition_parameter_2(bytes, 0);
        Self::set_raw_condition_run_on(bytes, CTDA_RUN_ON_SUBJECT);
        Self::set_raw_condition_reference(bytes, 0);
        Self::set_raw_condition_parameter_3(bytes, u32::MAX);
    }

    /// FO76 Story Manager event-data member ids for "the player this event is
    /// about". FO4 has no such members — its own Story Manager nodes only ever
    /// use `0x3152`, `0x3252` or `-1` on a Run On of Event Data (verified over
    /// `Fallout4.esm`: 122 event-data rows, 83/32/7 respectively). A carried-over
    /// FO76 id resolves to nothing, so the condition can never be true and the
    /// node can never select its quest.
    const FO76_STORY_MANAGER_PLAYER_EVENT_MEMBERS: [u32; 2] = [0x0000_3150, 0x0000_3352];

    /// Re-run-on the FO76 player event-data rows the targeted lowerings above
    /// did not already rewrite (they normalize only the rows whose *function*
    /// they change). `GetLevel`, `HasKeyword` and friends carry through with
    /// their function intact and their Run On still pointing at an FO76 member.
    ///
    /// Subject is the right target: for a Story Manager node these predicates ask
    /// about the actor the event is about, and every producer in this pipeline
    /// raises its event as `Keyword.SendStoryEvent(None, playerRef, playerRef)`.
    /// This is the same normalization `normalize_story_manager_quest_condition`
    /// applies on the paths it does cover.
    pub(super) fn normalize_fo76_story_manager_event_data_run_on(record: &mut Record) {
        if !matches!(&record.sig.0, b"SMQN" | b"SMBN" | b"SMEN") {
            return;
        }
        for entry in &mut record.fields {
            if !matches!(&entry.sig.0, b"CTDA" | b"CTDT") {
                continue;
            }
            let FieldValue::Bytes(bytes) = &mut entry.value else {
                continue;
            };
            if Self::raw_condition_run_on(bytes) != Some(CTDA_RUN_ON_EVENT_DATA) {
                continue;
            }
            let Some(member) = Self::raw_condition_parameter_3(bytes) else {
                continue;
            };
            if !Self::FO76_STORY_MANAGER_PLAYER_EVENT_MEMBERS.contains(&member) {
                continue;
            }
            Self::set_raw_condition_run_on(bytes, CTDA_RUN_ON_SUBJECT);
            Self::set_raw_condition_reference(bytes, 0);
            Self::set_raw_condition_parameter_3(bytes, u32::MAX);
        }
    }

    pub(super) fn normalize_fo76_story_manager_level_event_data(record: &mut Record) {
        if record.sig.0 != *b"SMQN" {
            return;
        }
        let mut parents = record.fields.iter().filter(|entry| entry.sig.0 == *b"PNAM");
        let has_exact_parent = parents.next().is_some_and(|entry| {
            Self::nested_form_id(&entry.value) == Some(FO4_INCREASE_LEVEL_STORY_MANAGER_ROOT)
        }) && parents.next().is_none();
        if !has_exact_parent {
            return;
        }
        for entry in &mut record.fields {
            if !matches!(&entry.sig.0, b"CTDA" | b"CTDT") {
                continue;
            }
            let FieldValue::Bytes(bytes) = &mut entry.value else {
                continue;
            };
            if bytes.len() != 32
                || Self::raw_condition_function_id(bytes)
                    != Some(FO76_GET_EVENT_DATA_CONDITION_FUNCTION_ID)
                || Self::raw_condition_parameter_1(bytes) != Some(FO76_EVENT_DATA_LEVEL_SELECTOR)
                || Self::raw_condition_parameter_2(bytes) != Some(0)
                || Self::raw_condition_run_on(bytes) != Some(CTDA_RUN_ON_SUBJECT)
                || Self::raw_condition_reference(bytes) != Some(0)
                || Self::raw_condition_parameter_3(bytes) != Some(u32::MAX)
            {
                continue;
            }
            Self::set_raw_condition_function_id(bytes, GET_LEVEL_CONDITION_FUNCTION_ID);
            bytes[10..12].fill(0);
            Self::set_raw_condition_parameter_1(bytes, 0);
            Self::normalize_story_manager_quest_condition(bytes);
        }
    }

    pub(super) fn normalize_fo76_raw_condition_functions(record: &mut Record) {
        Self::normalize_wayward_player_gender_conditions(record);
        Self::normalize_fo76_get_is_player_conditions(record);
        Self::normalize_fo76_editor_location_has_keyword_conditions(record);
        Self::normalize_fo76_quest_completion_conditions(record);
        Self::normalize_fo76_story_manager_level_event_data(record);
        Self::normalize_fo76_story_manager_event_data_run_on(record);
        let is_music_track = record.sig.0 == *b"MUST";
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
            } else if is_music_track
                && function_id == FO76_GET_STRONGEST_ENEMY_HAS_KEYWORD_CONDITION_FUNCTION_ID
            {
                Self::set_raw_condition_function_id(
                    bytes,
                    FO4_GET_COMBAT_TARGET_HAS_KEYWORD_CONDITION_FUNCTION_ID,
                );
            } else if function_id == FO76_GET_IS_CURRENT_LOCATION_EXACT_CONDITION_FUNCTION_ID {
                Self::set_raw_condition_function_id(
                    bytes,
                    FO4_GET_IN_CURRENT_LOCATION_CONDITION_FUNCTION_ID,
                );
            } else if matches!(
                function_id,
                FO76_GET_QUEST_RUNNING_UNIQUE_CONDITION_FUNCTION_ID
                    | FO76_IS_QUEST_ACTIVE_CONDITION_FUNCTION_ID
            ) {
                Self::set_raw_condition_function_id(
                    bytes,
                    FO4_GET_QUEST_RUNNING_CONDITION_FUNCTION_ID,
                );
            }
        }
    }

    /// FO4 resolves these FO76 `Target` conditions against Isela's scene target,
    /// not the player. Both the INFO ids and full CTDA shape are guarded because
    /// other dialogue legitimately uses `GetIsSex(Target)` for NPCs.
    pub(super) fn normalize_wayward_player_gender_conditions(record: &mut Record) {
        if record.sig.0 != *b"INFO"
            || !WAYWARD_PLAYER_GENDER_INFO_FORM_IDS.contains(&record.form_key.local)
        {
            return;
        }
        for entry in &mut record.fields {
            if entry.sig.0 != *b"CTDA" {
                continue;
            }
            let FieldValue::Bytes(bytes) = &mut entry.value else {
                continue;
            };
            let is_exact_player_gender_branch = bytes.len() == 32
                && bytes[..4] == [0, 0, 0, 0]
                && Self::raw_condition_comparison_value(bytes) == Some(1.0)
                && Self::raw_condition_function_id(bytes)
                    == Some(FO4_GET_IS_SEX_CONDITION_FUNCTION_ID)
                && bytes[10..12] == [0, 0]
                && matches!(Self::raw_condition_parameter_1(bytes), Some(0) | Some(1))
                && Self::raw_condition_parameter_2(bytes) == Some(0)
                && Self::raw_condition_run_on(bytes) == Some(CTDA_RUN_ON_TARGET)
                && Self::raw_condition_reference(bytes) == Some(0)
                && Self::raw_condition_parameter_3(bytes) == Some(u32::MAX);
            if !is_exact_player_gender_branch {
                continue;
            }
            Self::set_raw_condition_run_on(bytes, CTDA_RUN_ON_REFERENCE);
            Self::set_raw_condition_reference(bytes, FO4_PLAYER_REF_FORM_ID);
        }
    }

    pub(super) fn normalize_fo76_editor_location_has_keyword_conditions(record: &mut Record) {
        for entry in &mut record.fields {
            if !matches!(&entry.sig.0, b"CTDA" | b"CTDT") {
                continue;
            }
            let FieldValue::Bytes(bytes) = &mut entry.value else {
                continue;
            };
            if Self::raw_condition_function_id(bytes)
                == Some(FO76_EDITOR_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID)
            {
                Self::set_raw_condition_function_id(
                    bytes,
                    FO4_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID,
                );
            }
        }
    }

    /// Production `source_read` keeps `struct:` codecs, including CTDA/CTDT,
    /// as raw bytes. Synthetic structured conditions are deliberately left
    /// untouched because Parameter #1's union shape cannot be rewritten
    /// losslessly without knowing the exact authoring variant.
    pub(super) fn normalize_fo76_get_is_player_conditions(record: &mut Record) {
        for entry in &mut record.fields {
            if !matches!(&entry.sig.0, b"CTDA" | b"CTDT") {
                continue;
            }
            let FieldValue::Bytes(bytes) = &mut entry.value else {
                continue;
            };
            if Self::raw_condition_function_id(bytes)
                != Some(FO76_GET_IS_PLAYER_CONDITION_FUNCTION_ID)
            {
                continue;
            }
            Self::set_raw_condition_function_id(bytes, FO4_GET_IS_ID_CONDITION_FUNCTION_ID);
            Self::set_raw_condition_parameter_1(bytes, FO4_PLAYER_ACTOR_FORM_ID);
        }
    }

    pub(super) fn is_fo4_incompatible_condition_function_id(function_id: u16) -> bool {
        is_fo4_incompatible_condition_function_id(function_id)
    }

    pub(super) fn raw_condition_function_id(bytes: &[u8]) -> Option<u16> {
        if bytes.len() < 10 {
            return None;
        }
        Some(u16::from_le_bytes([bytes[8], bytes[9]]))
    }

    pub(super) fn set_raw_condition_function_id(bytes: &mut [u8], function_id: u16) {
        if bytes.len() >= 10 {
            bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        }
    }

    pub(super) fn raw_condition_parameter_1(bytes: &[u8]) -> Option<u32> {
        if bytes.len() < 16 {
            return None;
        }
        Some(u32::from_le_bytes([
            bytes[12], bytes[13], bytes[14], bytes[15],
        ]))
    }

    pub(super) fn set_raw_condition_parameter_1(bytes: &mut [u8], parameter_1: u32) {
        if bytes.len() >= 16 {
            bytes[12..16].copy_from_slice(&parameter_1.to_le_bytes());
        }
    }

    pub(super) fn raw_condition_parameter_2(bytes: &[u8]) -> Option<u32> {
        if bytes.len() < 20 {
            return None;
        }
        Some(u32::from_le_bytes([
            bytes[16], bytes[17], bytes[18], bytes[19],
        ]))
    }

    fn set_raw_condition_parameter_2(bytes: &mut [u8], parameter_2: u32) {
        if bytes.len() >= 20 {
            bytes[16..20].copy_from_slice(&parameter_2.to_le_bytes());
        }
    }

    fn set_raw_condition_run_on(bytes: &mut [u8], run_on: u32) {
        if bytes.len() >= 24 {
            bytes[20..24].copy_from_slice(&run_on.to_le_bytes());
        }
    }

    pub(super) fn raw_condition_reference(bytes: &[u8]) -> Option<u32> {
        if bytes.len() < 28 {
            return None;
        }
        Some(u32::from_le_bytes([
            bytes[24], bytes[25], bytes[26], bytes[27],
        ]))
    }

    fn set_raw_condition_reference(bytes: &mut [u8], reference: u32) {
        if bytes.len() >= 28 {
            bytes[24..28].copy_from_slice(&reference.to_le_bytes());
        }
    }

    pub(super) fn raw_condition_parameter_3(bytes: &[u8]) -> Option<u32> {
        if bytes.len() < 32 {
            return None;
        }
        Some(u32::from_le_bytes([
            bytes[28], bytes[29], bytes[30], bytes[31],
        ]))
    }

    fn set_raw_condition_parameter_3(bytes: &mut [u8], parameter_3: u32) {
        if bytes.len() >= 32 {
            bytes[28..32].copy_from_slice(&parameter_3.to_le_bytes());
        }
    }

    pub(super) fn condition_function_id(
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

    pub(super) fn condition_parameter_1(
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

    pub(super) fn raw_condition_run_on(bytes: &[u8]) -> Option<u32> {
        if bytes.len() < 24 {
            return None;
        }
        Some(u32::from_le_bytes([
            bytes[20], bytes[21], bytes[22], bytes[23],
        ]))
    }

    pub(super) fn condition_run_on(
        interner: &crate::sym::StringInterner,
        value: &FieldValue,
    ) -> Option<u32> {
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
    pub(super) fn raw_condition_operator(bytes: &[u8]) -> Option<u8> {
        bytes.first().map(|b| (b >> 5) & 0x07)
    }

    pub(super) fn condition_operator(
        interner: &crate::sym::StringInterner,
        value: &FieldValue,
    ) -> Option<u8> {
        match value {
            FieldValue::Bytes(bytes) => Self::raw_condition_operator(bytes),
            FieldValue::Struct(fields) => named_value_canonical(fields, "Type", interner)
                .and_then(field_value_to_u16)
                .map(|value| ((value as u8) >> 5) & 0x07),
            _ => None,
        }
    }

    pub(super) fn condition_uses_comparison_global(
        interner: &crate::sym::StringInterner,
        value: &FieldValue,
    ) -> Option<bool> {
        match value {
            FieldValue::Bytes(bytes) => bytes
                .first()
                .map(|value| value & CTDA_COMPARISON_GLOBAL_FLAG != 0),
            FieldValue::Struct(fields) => named_value_canonical(fields, "Type", interner)
                .and_then(field_value_to_u16)
                .map(|value| value as u8 & CTDA_COMPARISON_GLOBAL_FLAG != 0),
            _ => None,
        }
    }

    pub(super) fn raw_condition_comparison_value(bytes: &[u8]) -> Option<f32> {
        if bytes.len() < 8 || bytes[0] & CTDA_COMPARISON_GLOBAL_FLAG != 0 {
            return None;
        }
        Some(f32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]))
    }

    pub(super) fn condition_comparison_value(
        interner: &crate::sym::StringInterner,
        value: &FieldValue,
    ) -> Option<f32> {
        match value {
            FieldValue::Bytes(bytes) => Self::raw_condition_comparison_value(bytes),
            FieldValue::Struct(fields) => {
                if Self::condition_uses_comparison_global(interner, value)? {
                    return None;
                }
                named_value_canonical(fields, "ComparisonValue", interner)
                    .and_then(Self::nested_f32)
            }
            _ => None,
        }
    }

    pub(super) fn condition_comparison_global_id(
        interner: &crate::sym::StringInterner,
        value: &FieldValue,
    ) -> Option<u32> {
        if !Self::condition_uses_comparison_global(interner, value)? {
            return None;
        }
        match value {
            FieldValue::Bytes(bytes) if bytes.len() >= 8 => {
                Some(u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) & 0x00FF_FFFF)
            }
            FieldValue::Struct(fields) => {
                named_value_canonical(fields, "ComparisonValue", interner)
                    .and_then(Self::nested_form_id)
            }
            _ => None,
        }
    }

    pub(super) fn nested_f32(value: &FieldValue) -> Option<f32> {
        match value {
            FieldValue::Float(value) => Some(*value),
            FieldValue::Uint(value) => Some(*value as f32),
            FieldValue::Int(value) => Some(*value as f32),
            FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
                Some(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            }
            FieldValue::List(values) => values.iter().find_map(Self::nested_f32),
            FieldValue::Struct(fields) => {
                fields.iter().find_map(|(_, value)| Self::nested_f32(value))
            }
            _ => None,
        }
    }

    pub(super) fn nested_form_id(value: &FieldValue) -> Option<u32> {
        match value {
            FieldValue::FormKey(form_key) => Some(form_key.local & 0x00FF_FFFF),
            FieldValue::Uint(value) => u32::try_from(*value).ok().map(|value| value & 0x00FF_FFFF),
            FieldValue::Int(value) => u32::try_from(*value).ok().map(|value| value & 0x00FF_FFFF),
            FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
                Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) & 0x00FF_FFFF)
            }
            FieldValue::List(values) => values.iter().find_map(Self::nested_form_id),
            FieldValue::Struct(fields) => fields
                .iter()
                .find_map(|(_, value)| Self::nested_form_id(value)),
            _ => None,
        }
    }

    pub(super) fn numeric_condition_matches(value: f32, operator: u8, comparison: f32) -> bool {
        match operator {
            0 => value == comparison,
            1 => value != comparison,
            2 => value > comparison,
            3 => value >= comparison,
            4 => value < comparison,
            5 => value <= comparison,
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// FO76 condition forms (CNDF): function-875 inlining
// ---------------------------------------------------------------------------
//
// FO76 has a reusable named condition record, `CNDF`: an EDID plus a CTDA list
// that other records point at instead of repeating the conditions. Condition
// function 875 dereferences one: `Fn875(<CNDF>) == 1` means "the condition
// form holds", `== 0` means "it does not".
//
// FO4 has neither the record type nor the function, so such a CTDA is dropped
// as an incompatible function id. On an ANDed slot that makes the owning record
// MORE permissive than authored (a dialogue INFO plays when the source would
// not). Instead, the referenced form's own conditions are spliced in place of
// the 875 row. Whether that is faithful depends on how FO4 groups a flat CTDA
// list.
//
// # The FO4 grouping model
//
// A CTDA list has no parentheses. Grouping is carried by one bit, `Or`
// (`flags & 0x01`): **a maximal run of consecutive Or-flagged rows is one OR
// group; every group and every unflagged row is ANDed together.**
//
// Measured on `Fallout4.esm`: of 2,457 maximal Or runs, 1,672 end at the end of
// their list and 989 start at its start. Reading Or as "OR with the next row"
// leaves every list-ending run dangling (68% of runs); "OR with the previous
// row" leaves every list-starting run dangling (40%). The run model has neither
// anomaly, and a length-1 run degenerates to a plain ANDed row, which is what
// the 420 length-1 runs need to mean. `SeventySix.esm` shows the same
// two-sided distribution.
//
// Two consequences drive everything below:
//   * an unflagged row is a *separator*: it cannot merge with a neighbouring
//     run, so replacing one unflagged row with several unflagged rows never
//     disturbs the groups around it;
//   * a flagged row at a block boundary CAN merge with a flagged neighbour, so
//     splicing a form whose first/last row is flagged next to a flagged host
//     row silently widens that host group.
//
// # Composition rules
//
// With `S` = the (recursively expanded) rows of the referenced form, `flag` =
// the Or flag of the 875 row it replaces, and `pos` = its polarity:
//
//   * `|S| == 1` — substitute, carrying `flag` onto the single row (and
//     inverting its operator when `!pos`). Structure is untouched, so this is
//     exact regardless of neighbours or of `S`'s own flag.
//   * `pos`, `|S| > 1`, `flag` clear — splice `S` verbatim into the AND slot.
//     `S`'s internal AND/OR structure is preserved bit-for-bit; refused when a
//     flagged first/last row of `S` would merge with a flagged host neighbour.
//   * `pos`, `|S| > 1`, `flag` set — REFUSED. The slot is one alternative of a
//     host OR group and the replacement is a conjunction; `A || (b && c)` is an
//     OR of an AND and the flat model cannot express it.
//   * `!pos`, `|S| > 1`, `S` all-AND — De Morgan: emit `|S|` rows, every
//     operator inverted and every row Or-flagged, i.e. `¬s0 || ¬s1 || …`. When
//     `flag` is set those rows join the host OR group, which is exactly the
//     alternative the 875 contributed; when it is clear they form their own
//     isolated group, refused if it would merge with a flagged neighbour.
//   * `!pos`, `|S| > 1`, `S` contains an OR group — REFUSED. `¬(a && (b||c))`
//     is `¬a || (¬b && ¬c)`: an OR of an AND again, inexpressible.
//
// Everything else is refused rather than approximated, and every refusal is
// traced under stage `fo76_fo4.conditions` so a drop-trace run shows the exact
// shape that could not be carried. A silently wrong condition is far worse
// than a traced drop.
//
// # Other refusals
//
//   * the 875 row's Run On is not Subject, or its Reference is non-zero — the
//     form would have to be re-evaluated against another object and the inner
//     rows carry their own Run On, which cannot be composed faithfully;
//   * the 875 row is not a plain boolean test (`== 1` / `!= 0` / `== 0` /
//     `!= 1`), including a Global-backed comparison value;
//   * a cycle through the CNDF graph (forms do reference other forms);
//   * an expanded row that FO4 itself cannot evaluate — an FO76-only condition
//     function, or a Run On above FO4's maximum of 7. Inlining those would
//     just move the drop one level down, and dropping one member of an OR
//     group makes a record more RESTRICTIVE, which can suppress content;
//   * a form carrying CIS1/CIS2 parameter strings — those are positional and
//     would have to move with their row.
//
// # Where this runs
//
// `pre_translate`, before any condition normalization. The spliced rows are
// then indistinguishable from the host record's own conditions: they go
// through the same function remaps, the same FO4-incompatibility drop, the same
// CITC resync, and the same `remap_struct_internal_formids` FormID remap.

/// FO76 `GetConditionFormResult` — dereferences a `CNDF` named condition form
/// named in Parameter #1. Above FO4's max known function id, so untouched it is
/// always dropped.
///
/// The generated FO76 schema types Parameter #1 of this function as a
/// `CLAS` formlink. That is wrong — every observed target is a `CNDF`
/// (`SeventySix.esm` holds 883 CNDF against 73 CLAS, and all 4,166 observed
/// Parameter #1 values resolve to CNDF object ids). Resolve it as CNDF.
pub(super) const FO76_CONDITION_FORM_CONDITION_FUNCTION_ID: u16 = 875;
const CTDA_OR_FLAG: u8 = 0x01;
const CTDA_ROW_LEN: usize = 32;
/// FO4's condition "Run On" enum tops out at 7 (`Event Data`).
const FO4_MAX_CONDITION_RUN_ON: u32 = 7;

/// One FO76 named condition form.
struct ConditionForm {
    rows: Vec<Vec<u8>>,
    /// CIS1/CIS2 parameter strings are positional — they would have to travel
    /// with their row, so a form carrying them is never spliced.
    has_parameter_strings: bool,
}

/// Source-plugin `CNDF` records, indexed by object id.
pub(crate) struct ConditionFormCatalog {
    /// The master-list index a source record refers to itself by, i.e. the high
    /// byte of an in-plugin FormID. Guards against reading a master's record id
    /// as if it were an own-plugin CNDF.
    own_master_index: u8,
    forms: rustc_hash::FxHashMap<u32, ConditionForm>,
}

impl ConditionFormCatalog {
    pub(crate) fn new(own_master_index: u8) -> Self {
        Self {
            own_master_index,
            forms: rustc_hash::FxHashMap::default(),
        }
    }

    pub(crate) fn insert(
        &mut self,
        object_id: u32,
        rows: Vec<Vec<u8>>,
        has_parameter_strings: bool,
    ) {
        self.forms.insert(
            object_id & 0x00FF_FFFF,
            ConditionForm {
                rows,
                has_parameter_strings,
            },
        );
    }

    pub(crate) fn len(&self) -> usize {
        self.forms.len()
    }

    fn get(&self, raw_form_id: u32) -> Option<&ConditionForm> {
        if (raw_form_id >> 24) as u8 != self.own_master_index {
            return None;
        }
        self.forms.get(&(raw_form_id & 0x00FF_FFFF))
    }
}

fn catalog_slot() -> &'static RwLock<Option<Arc<ConditionFormCatalog>>> {
    static SLOT: OnceLock<RwLock<Option<Arc<ConditionFormCatalog>>>> = OnceLock::new();
    SLOT.get_or_init(|| RwLock::new(None))
}

/// Publish the catalog for the run's pair hooks. The hook sees only a `Record`,
/// so the cross-record CNDF lookup has to reach it out of band.
pub(crate) fn install_condition_form_catalog(catalog: ConditionFormCatalog) {
    if let Ok(mut slot) = catalog_slot().write() {
        *slot = Some(Arc::new(catalog));
    }
}

pub(crate) fn condition_form_catalog() -> Option<Arc<ConditionFormCatalog>> {
    catalog_slot().read().ok().and_then(|slot| slot.clone())
}

#[cfg(test)]
pub(crate) fn clear_condition_form_catalog() {
    if let Ok(mut slot) = catalog_slot().write() {
        *slot = None;
    }
}

/// Index every `CNDF` in the source plugin. Raw subrecord bytes are used rather
/// than the decoded record so an incomplete FO76 schema entry for CNDF cannot
/// silently empty the catalog.
pub(crate) fn build_condition_form_catalog(
    source_handle_id: u64,
    interner: &crate::sym::StringInterner,
) -> Result<ConditionFormCatalog, String> {
    let sig = crate::ids::SigCode::from_str("CNDF").map_err(|err| err.to_string())?;
    let (_, masters) = crate::source_read::plugin_context_for_handle(source_handle_id)
        .map_err(|err| err.to_string())?;
    let own_master_index = u8::try_from(masters.len()).unwrap_or(0);
    let mut catalog = ConditionFormCatalog::new(own_master_index);
    let form_keys = crate::source_read::iter_form_keys_of_sig(source_handle_id, sig, interner)
        .map_err(|err| err.to_string())?;
    if form_keys.is_empty() {
        return Ok(catalog);
    }
    let snapshot =
        crate::source_read::snapshot_records_by_form_keys(source_handle_id, &form_keys, interner)
            .map_err(|err| err.to_string())?;
    for record in &snapshot.records {
        let mut rows = Vec::new();
        let mut has_parameter_strings = false;
        for subrecord in effective_subrecords_for_record(&record.raw_record).iter() {
            match subrecord.signature.as_str() {
                "CTDA" | "CTDT" => rows.push(subrecord.data.to_vec()),
                "CIS1" | "CIS2" => has_parameter_strings = true,
                _ => {}
            }
        }
        catalog.insert(record.form_key.local, rows, has_parameter_strings);
    }
    Ok(catalog)
}

fn condition_row_is_ored(row: &[u8]) -> bool {
    row.first().is_some_and(|flags| flags & CTDA_OR_FLAG != 0)
}

fn condition_row_with_or_flag(row: &[u8], ored: bool) -> Vec<u8> {
    let mut out = row.to_vec();
    if let Some(flags) = out.first_mut() {
        if ored {
            *flags |= CTDA_OR_FLAG;
        } else {
            *flags &= !CTDA_OR_FLAG;
        }
    }
    out
}

/// `NOT (value <op> comparison)` expressed as `value <op'> comparison`.
/// Every CTDA operator has an exact complement, so this never approximates.
fn negated_condition_row(row: &[u8]) -> Option<Vec<u8>> {
    let complement = match Fo76Fo4Hook::raw_condition_operator(row)? {
        0 => 1,
        1 => 0,
        2 => 5,
        3 => 4,
        4 => 3,
        5 => 2,
        _ => return None,
    };
    let mut out = row.to_vec();
    out[0] = (out[0] & 0x1F) | (complement << 5);
    Some(out)
}

/// `Some(true)` when the row asserts the condition form holds, `Some(false)`
/// when it asserts it does not, `None` when the row is not a plain boolean test
/// of the form result and therefore cannot be carried.
fn condition_form_polarity(row: &[u8]) -> Option<bool> {
    let operator = Fo76Fo4Hook::raw_condition_operator(row)?;
    // `raw_condition_comparison_value` already returns None for a Global-backed
    // comparison, which has no compile-time truth value to match on.
    let comparison = Fo76Fo4Hook::raw_condition_comparison_value(row)?;
    match (operator, comparison) {
        (0, value) if value == 1.0 => Some(true),
        (0, value) if value == 0.0 => Some(false),
        (1, value) if value == 0.0 => Some(true),
        (1, value) if value == 1.0 => Some(false),
        _ => None,
    }
}

/// An expanded row plus, for a row that came through untouched, its index in
/// the host block so its CIS1/CIS2 parameter strings can be reattached.
struct ExpandedRow {
    origin: Option<usize>,
    bytes: Vec<u8>,
}

/// Refuse an inlined row FO4 cannot evaluate. Splicing it would only move the
/// drop one level down — and inside an OR group a dropped member removes an
/// alternative, which makes the record more restrictive than authored.
fn unusable_inlined_row(row: &[u8]) -> Option<String> {
    let function_id = Fo76Fo4Hook::raw_condition_function_id(row)?;
    // The spliced rows run through `normalize_fo76_raw_condition_functions`
    // like the host's own, so an id that pass rewrites is usable. 857
    // (`GetNumTimesCompletedQuest`) is lowered there too, but only for the
    // predicates that carry onto FO4's boolean `GetQuestCompleted`.
    let remapped = FO76_REMAPPED_CONDITION_FUNCTION_IDS.contains(&function_id)
        || function_id == FO76_GET_NUM_TIMES_COMPLETED_QUEST_CONDITION_FUNCTION_ID;
    if !remapped && is_fo4_incompatible_condition_function_id(function_id) {
        return Some(format!(
            "inlined condition function {function_id} has no FO4 slot"
        ));
    }
    let run_on = Fo76Fo4Hook::raw_condition_run_on(row)?;
    (run_on > FO4_MAX_CONDITION_RUN_ON)
        .then(|| format!("inlined condition run_on={run_on} has no FO4 slot"))
}

/// Splice one 875 slot. `previous`/`next` are the rows that will neighbour the
/// splice in the host block; they decide whether a flagged boundary row would
/// merge into a neighbouring OR group.
fn compose_condition_form(
    expanded: &[Vec<u8>],
    slot_is_ored: bool,
    positive: bool,
    previous: Option<&[u8]>,
    next: Option<&[u8]>,
) -> Result<Vec<Vec<u8>>, String> {
    if expanded.is_empty() {
        return Err("condition form has no conditions".to_string());
    }
    if expanded.len() == 1 {
        let row = if positive {
            expanded[0].clone()
        } else {
            negated_condition_row(&expanded[0])
                .ok_or_else(|| "condition operator has no complement".to_string())?
        };
        return Ok(vec![condition_row_with_or_flag(&row, slot_is_ored)]);
    }
    if positive {
        if slot_is_ored {
            return Err(
                "multi-condition form in an OR slot would need an AND inside an OR group"
                    .to_string(),
            );
        }
        if condition_row_is_ored(&expanded[0]) && previous.is_some_and(condition_row_is_ored) {
            return Err("spliced OR group would merge with the preceding host group".to_string());
        }
        if condition_row_is_ored(expanded.last().unwrap())
            && next.is_some_and(condition_row_is_ored)
        {
            return Err("spliced OR group would merge with the following host group".to_string());
        }
        return Ok(expanded.to_vec());
    }
    // Negation: De Morgan turns the form's AND list into an OR group, so the
    // form must not already contain one.
    if expanded.iter().any(|row| condition_row_is_ored(row)) {
        return Err(
            "negating a form that contains an OR group would need an OR of ANDs".to_string(),
        );
    }
    if !slot_is_ored {
        if previous.is_some_and(condition_row_is_ored) {
            return Err("negated OR group would merge with the preceding host group".to_string());
        }
        if next.is_some_and(condition_row_is_ored) {
            return Err("negated OR group would merge with the following host group".to_string());
        }
    }
    expanded
        .iter()
        .map(|row| {
            negated_condition_row(row)
                .map(|negated| condition_row_with_or_flag(&negated, true))
                .ok_or_else(|| "condition operator has no complement".to_string())
        })
        .collect()
}

/// Fully expand every 875 row in `rows`, recursing through nested condition
/// forms. `path` carries the forms currently being expanded so a cycle is
/// reported instead of overflowing the stack.
fn expand_condition_form_rows(
    rows: &[Vec<u8>],
    catalog: &ConditionFormCatalog,
    path: &mut Vec<u32>,
    top_level: bool,
) -> Result<Vec<ExpandedRow>, String> {
    let mut out: Vec<ExpandedRow> = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate() {
        if row.len() != CTDA_ROW_LEN
            || Fo76Fo4Hook::raw_condition_function_id(row)
                != Some(FO76_CONDITION_FORM_CONDITION_FUNCTION_ID)
        {
            out.push(ExpandedRow {
                origin: top_level.then_some(index),
                bytes: row.clone(),
            });
            continue;
        }
        let run_on = Fo76Fo4Hook::raw_condition_run_on(row).unwrap_or(u32::MAX);
        if run_on != CTDA_RUN_ON_SUBJECT {
            return Err(format!("condition-form row run_on={run_on} is not Subject"));
        }
        if Fo76Fo4Hook::raw_condition_reference(row) != Some(0) {
            return Err("condition-form row targets an explicit reference".to_string());
        }
        let positive = condition_form_polarity(row)
            .ok_or_else(|| "condition-form row is not a plain boolean test".to_string())?;
        let target = Fo76Fo4Hook::raw_condition_parameter_1(row).unwrap_or(0);
        let form = catalog
            .get(target)
            .ok_or_else(|| format!("condition form {target:08X} is not in the source plugin"))?;
        if form.has_parameter_strings {
            return Err(format!(
                "condition form {target:08X} carries CIS1/CIS2 parameter strings"
            ));
        }
        if path.contains(&target) {
            return Err(format!("cycle through condition form {target:08X}"));
        }
        path.push(target);
        let expanded = expand_condition_form_rows(&form.rows, catalog, path, false)?;
        path.pop();
        let expanded: Vec<Vec<u8>> = expanded.into_iter().map(|row| row.bytes).collect();
        for expanded_row in &expanded {
            if let Some(reason) = unusable_inlined_row(expanded_row) {
                return Err(reason);
            }
        }
        let previous = rows.get(index.wrapping_sub(1)).filter(|_| index > 0);
        let next = rows.get(index + 1);
        let spliced = compose_condition_form(
            &expanded,
            condition_row_is_ored(row),
            positive,
            previous.map(|row| row.as_slice()),
            next.map(|row| row.as_slice()),
        )?;
        out.extend(spliced.into_iter().map(|bytes| ExpandedRow {
            origin: None,
            bytes,
        }));
    }
    Ok(out)
}

impl Fo76Fo4Hook {
    /// Replace every FO76 condition-form dereference (function 875) with the
    /// referenced `CNDF`'s own conditions, so the gate survives into FO4
    /// instead of being dropped with the function.
    ///
    /// Refusals leave the 875 row in place; the incompatible-function pass then
    /// drops and traces it.
    pub(super) fn inline_fo76_condition_forms(record: &mut Record) {
        let has_condition_form = record.fields.iter().any(|entry| {
            matches!(&entry.sig.0, b"CTDA" | b"CTDT")
                && matches!(&entry.value, FieldValue::Bytes(bytes)
                    if Self::raw_condition_function_id(bytes)
                        == Some(FO76_CONDITION_FORM_CONDITION_FUNCTION_ID))
        });
        if !has_condition_form {
            return;
        }
        let Some(catalog) = condition_form_catalog() else {
            return;
        };
        let record_sig = record.sig.0;
        let record_local = record.form_key.local;
        let trace = crate::drop_trace::enabled();
        let fields = std::mem::take(&mut record.fields);
        let mut out: SmallVec<[FieldEntry; 8]> = SmallVec::new();
        let mut index = 0;
        let mut changed = false;
        while index < fields.len() {
            if !matches!(&fields[index].sig.0, b"CTDA" | b"CTDT") {
                out.push(fields[index].clone());
                index += 1;
                continue;
            }
            let start = index;
            let mut end = index;
            while end < fields.len()
                && matches!(&fields[end].sig.0, b"CTDA" | b"CTDT" | b"CIS1" | b"CIS2")
            {
                end += 1;
            }
            index = end;
            let block = &fields[start..end];
            let mut rows: Vec<Vec<u8>> = Vec::new();
            let mut attachments: Vec<Vec<FieldEntry>> = Vec::new();
            let mut row_sigs: Vec<SubrecordSig> = Vec::new();
            let mut readable = true;
            for entry in block {
                match &entry.sig.0 {
                    b"CTDA" | b"CTDT" => match &entry.value {
                        FieldValue::Bytes(bytes) if bytes.len() == CTDA_ROW_LEN => {
                            rows.push(bytes.to_vec());
                            row_sigs.push(entry.sig);
                            attachments.push(Vec::new());
                        }
                        // A structured or short condition can't be spliced
                        // byte-wise; leave the whole block alone.
                        _ => {
                            readable = false;
                            break;
                        }
                    },
                    _ => match attachments.last_mut() {
                        Some(slot) => slot.push(entry.clone()),
                        None => {
                            readable = false;
                            break;
                        }
                    },
                }
            }
            let expansion = readable
                .then(|| {
                    rows.iter().any(|row| {
                        Self::raw_condition_function_id(row)
                            == Some(FO76_CONDITION_FORM_CONDITION_FUNCTION_ID)
                    })
                })
                .unwrap_or(false)
                .then(|| expand_condition_form_rows(&rows, &catalog, &mut Vec::new(), true));
            match expansion {
                Some(Ok(expanded)) => {
                    changed = true;
                    let default_sig = row_sigs[0];
                    for row in expanded {
                        let sig = row.origin.map_or(default_sig, |origin| row_sigs[origin]);
                        out.push(FieldEntry {
                            sig,
                            value: FieldValue::Bytes(SmallVec::from_slice(&row.bytes)),
                        });
                        if let Some(origin) = row.origin {
                            out.extend(attachments[origin].iter().cloned());
                        }
                    }
                    if trace {
                        crate::drop_trace::trace(
                            "fo76_fo4.conditions",
                            std::str::from_utf8(&record_sig).unwrap_or("????"),
                            record_local,
                            "CTDA",
                            "FO76 condition form(s) (function 875) inlined into the condition block",
                        );
                    }
                }
                Some(Err(reason)) => {
                    if trace {
                        crate::drop_trace::trace(
                            "fo76_fo4.conditions",
                            std::str::from_utf8(&record_sig).unwrap_or("????"),
                            record_local,
                            "CTDA",
                            &format!(
                                "FO76 condition form (function 875) NOT inlined; {reason}; \
                                 the condition will be dropped"
                            ),
                        );
                    }
                    out.extend(block.iter().cloned());
                }
                None => out.extend(block.iter().cloned()),
            }
        }
        record.fields = out;
        if changed && record.fields.iter().any(|entry| entry.sig.0 == *b"CITC") {
            record.sync_condition_count();
        }
    }
}
