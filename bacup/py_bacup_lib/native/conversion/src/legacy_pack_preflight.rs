use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::ids::FormKey;
use crate::record::Record;
use crate::sym::StringInterner;
use crate::translator::pair_hooks::fnv_pack::{
    AUDITED_FNV_PACK_COUNT, AUDITED_FO3_PACK_COUNT, AUDITED_LEGACY_PACK_COUNT,
    LegacyPackClassificationStatus, LegacyPackCorpusReport, LegacyPackLoweringBlocker,
    LegacyPackRejectionReason, LegacyPackSourceFamily, LegacyPackType, classify_legacy_pack,
    legacy_pack_type_hint,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyPackOriginRow {
    pub merged_form_key: String,
    pub source_game: String,
    pub source_plugin: String,
    pub source_form_key: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyPackExpectedCounts {
    pub fnv: usize,
    pub fo3: usize,
}

impl LegacyPackExpectedCounts {
    pub const fn audited_merged() -> Self {
        Self {
            fnv: AUDITED_FNV_PACK_COUNT,
            fo3: AUDITED_FO3_PACK_COUNT,
        }
    }

    pub const fn audited_for(source: LegacyPackSourceFamily) -> Self {
        match source {
            LegacyPackSourceFamily::Fnv => Self {
                fnv: AUDITED_FNV_PACK_COUNT,
                fo3: 0,
            },
            LegacyPackSourceFamily::Fo3 => Self {
                fnv: 0,
                fo3: AUDITED_FO3_PACK_COUNT,
            },
            LegacyPackSourceFamily::Fo76 | LegacyPackSourceFamily::Fo4 => Self { fnv: 0, fo3: 0 },
        }
    }

    pub const fn total(self) -> usize {
        self.fnv + self.fo3
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LegacyPackBlockedRecord {
    pub source_family: Option<LegacyPackSourceFamily>,
    pub source_plugin: Option<String>,
    pub source_form_key: Option<String>,
    pub merged_form_key: String,
    pub editor_id: Option<String>,
    pub package_type_code: Option<u8>,
    pub package_type: Option<LegacyPackType>,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LegacyPackPreflightReport {
    pub raw_expected: LegacyPackExpectedCounts,
    pub raw_source_counts: LegacyPackExpectedCounts,
    pub exact_raw_source_coverage: bool,
    pub expected: LegacyPackExpectedCounts,
    pub observed_pack_records: usize,
    pub classified: LegacyPackCorpusReport,
    pub exact_expected_coverage: bool,
    pub provenance_errors: Vec<String>,
    pub blocker_counts: BTreeMap<String, usize>,
    pub blocked_records: Vec<LegacyPackBlockedRecord>,
    pub explicitly_excluded: bool,
    pub explicitly_excluded_records: usize,
}

impl LegacyPackPreflightReport {
    pub fn is_blocked(&self) -> bool {
        !self.exact_raw_source_coverage
            || !self.exact_expected_coverage
            || !self.provenance_errors.is_empty()
            || !self.blocked_records.is_empty()
    }

    pub fn error_summary_json(&self) -> String {
        serde_json::json!({
            "raw_expected": self.raw_expected,
            "raw_source_counts": self.raw_source_counts,
            "exact_raw_source_coverage": self.exact_raw_source_coverage,
            "expected": self.expected,
            "observed_pack_records": self.observed_pack_records,
            "classified": self.classified,
            "exact_expected_coverage": self.exact_expected_coverage,
            "provenance_error_count": self.provenance_errors.len(),
            "blocker_counts": self.blocker_counts,
            "blocked_record_count": self.blocked_records.len(),
            "first_blocked_record": self.blocked_records.first(),
            "explicitly_excluded": self.explicitly_excluded,
            "explicitly_excluded_records": self.explicitly_excluded_records,
        })
        .to_string()
    }
}

#[derive(Debug, Clone)]
struct ParsedOrigin {
    family: LegacyPackSourceFamily,
    row: LegacyPackOriginRow,
}

#[derive(Debug, Clone)]
pub struct DirectLegacyPackOrigin {
    pub family: LegacyPackSourceFamily,
    pub source_plugin: String,
}

pub struct LegacyPackPreflightAccumulator {
    raw_expected: LegacyPackExpectedCounts,
    raw_source_counts: Option<LegacyPackExpectedCounts>,
    expected: LegacyPackExpectedCounts,
    explicit_origins: HashMap<(String, u32), ParsedOrigin>,
    direct_origin: Option<DirectLegacyPackOrigin>,
    provenance_errors: Vec<String>,
    blocker_counts: BTreeMap<String, usize>,
    blocked_records: Vec<LegacyPackBlockedRecord>,
    observed_pack_records: usize,
    classified: LegacyPackCorpusReport,
    explicitly_excluded: bool,
    explicitly_excluded_records: usize,
}

impl LegacyPackPreflightAccumulator {
    pub fn new(
        origins: &[LegacyPackOriginRow],
        raw_expected: LegacyPackExpectedCounts,
        raw_source_counts: Option<LegacyPackExpectedCounts>,
        expected: LegacyPackExpectedCounts,
        direct_origin: Option<DirectLegacyPackOrigin>,
        require_explicit_origins: bool,
        explicitly_excluded: bool,
    ) -> Self {
        let mut explicit_origins = HashMap::with_capacity(origins.len());
        let mut source_identities = HashSet::with_capacity(origins.len());
        let mut provenance_errors = Vec::new();
        for row in origins {
            let Some(family) = source_family(&row.source_game) else {
                provenance_errors.push(format!(
                    "invalid_source_game:{}:{}",
                    row.merged_form_key, row.source_game
                ));
                continue;
            };
            let Some(key) = normalized_form_key(&row.merged_form_key) else {
                provenance_errors.push(format!("invalid_merged_form_key:{}", row.merged_form_key));
                continue;
            };
            let source_plugin = row.source_plugin.trim();
            let Some(source_key) = normalized_form_key(&row.source_form_key) else {
                provenance_errors.push(format!(
                    "invalid_source_identity:{}:{}",
                    row.merged_form_key, row.source_form_key
                ));
                continue;
            };
            if source_plugin.is_empty() || source_key.0 != source_plugin.to_ascii_lowercase() {
                provenance_errors.push(format!(
                    "invalid_source_identity:{}:{}",
                    row.merged_form_key, row.source_form_key
                ));
                continue;
            }
            if !source_identities.insert(source_key) {
                provenance_errors
                    .push(format!("duplicate_source_form_key:{}", row.source_form_key));
            }
            let parsed = ParsedOrigin {
                family,
                row: row.clone(),
            };
            if explicit_origins.insert(key, parsed).is_some() {
                provenance_errors
                    .push(format!("duplicate_merged_form_key:{}", row.merged_form_key));
            }
        }
        if require_explicit_origins && origins.is_empty() {
            provenance_errors.push("missing_explicit_pack_provenance".to_string());
        }
        Self {
            raw_expected,
            raw_source_counts,
            expected,
            explicit_origins,
            direct_origin,
            provenance_errors,
            blocker_counts: BTreeMap::new(),
            blocked_records: Vec::new(),
            observed_pack_records: 0,
            classified: LegacyPackCorpusReport {
                total_records: 0,
                fnv_records: 0,
                fo3_records: 0,
                accepted_records: 0,
                rejected_records: 0,
                by_type: BTreeMap::new(),
                exact_audited_coverage: false,
            },
            explicitly_excluded,
            explicitly_excluded_records: 0,
        }
    }

    pub fn observe_decoded(&mut self, record: &Record, interner: &StringInterner) {
        self.observed_pack_records += 1;
        let merged_form_key = rendered_form_key(record.form_key, interner);
        let origin = self.take_origin(record.form_key, interner);
        let Some(origin) = origin else {
            self.increment("missing_provenance");
            self.blocked_records.push(LegacyPackBlockedRecord {
                source_family: None,
                source_plugin: None,
                source_form_key: None,
                merged_form_key,
                editor_id: resolved_editor_id(record, interner),
                package_type_code: None,
                package_type: None,
                blockers: vec!["missing_provenance".to_string()],
            });
            return;
        };

        self.classified.total_records += 1;
        match origin.family {
            LegacyPackSourceFamily::Fnv => self.classified.fnv_records += 1,
            LegacyPackSourceFamily::Fo3 => self.classified.fo3_records += 1,
            LegacyPackSourceFamily::Fo76 | LegacyPackSourceFamily::Fo4 => {}
        }
        if self.explicitly_excluded {
            self.explicitly_excluded_records += 1;
            return;
        }

        let classification = classify_legacy_pack(record, origin.family, interner);

        let mut blockers = Vec::new();
        let mut package_type = legacy_pack_type_hint(record, interner);
        match classification.status {
            LegacyPackClassificationStatus::Accepted => {
                self.classified.accepted_records += 1;
                let inventory = classification
                    .inventory
                    .as_ref()
                    .expect("accepted PACK classification carries inventory");
                package_type = Some((inventory.package_type_code, inventory.package_type));
                *self
                    .classified
                    .by_type
                    .entry(inventory.package_type_code)
                    .or_default() += 1;
                for blocker in &inventory.support.lowering_blockers {
                    let code = lowering_blocker_code(blocker);
                    self.increment(code);
                    blockers.push(code.to_string());
                }
                if inventory.support.lowering_supported {
                    blockers.clear();
                }
            }
            LegacyPackClassificationStatus::Rejected => {
                self.classified.rejected_records += 1;
                for rejection in &classification.rejection_reasons {
                    let code = rejection_code(rejection);
                    self.increment(code);
                    blockers.push(code.to_string());
                }
            }
            LegacyPackClassificationStatus::NotApplicable => {
                self.increment("classifier_not_applicable");
                blockers.push("classifier_not_applicable".to_string());
            }
        }

        if !blockers.is_empty() {
            let (package_type_code, package_type) = package_type
                .map(|(code, package_type)| (Some(code), Some(package_type)))
                .unwrap_or((None, None));
            self.blocked_records.push(LegacyPackBlockedRecord {
                source_family: Some(origin.family),
                source_plugin: Some(origin.row.source_plugin),
                source_form_key: Some(origin.row.source_form_key),
                merged_form_key,
                editor_id: resolved_editor_id(record, interner),
                package_type_code,
                package_type,
                blockers,
            });
        }
    }

    pub fn observe_decode_error(
        &mut self,
        form_key: FormKey,
        message: String,
        interner: &StringInterner,
    ) {
        self.observed_pack_records += 1;
        let merged_form_key = rendered_form_key(form_key, interner);
        let origin = self.take_origin(form_key, interner);
        if self.explicitly_excluded
            && let Some(origin) = &origin
        {
            self.classified.total_records += 1;
            match origin.family {
                LegacyPackSourceFamily::Fnv => self.classified.fnv_records += 1,
                LegacyPackSourceFamily::Fo3 => self.classified.fo3_records += 1,
                LegacyPackSourceFamily::Fo76 | LegacyPackSourceFamily::Fo4 => {}
            }
            self.explicitly_excluded_records += 1;
            return;
        }
        self.increment("decode_error");
        let mut blockers = vec![format!("decode_error:{message}")];
        if origin.is_none() {
            self.increment("missing_provenance");
            blockers.push("missing_provenance".to_string());
        }
        self.blocked_records.push(LegacyPackBlockedRecord {
            source_family: origin.as_ref().map(|origin| origin.family),
            source_plugin: origin
                .as_ref()
                .map(|origin| origin.row.source_plugin.clone()),
            source_form_key: origin
                .as_ref()
                .map(|origin| origin.row.source_form_key.clone()),
            merged_form_key,
            editor_id: None,
            package_type_code: None,
            package_type: None,
            blockers,
        });
    }

    pub fn finish(mut self) -> LegacyPackPreflightReport {
        let stale_origins = std::mem::take(&mut self.explicit_origins);
        for origin in stale_origins.into_values() {
            self.increment("stale_provenance");
            self.provenance_errors.push(format!(
                "stale_provenance:{}:{}",
                origin.row.merged_form_key, origin.row.source_form_key
            ));
        }
        self.classified.exact_audited_coverage = self.classified.total_records
            == AUDITED_LEGACY_PACK_COUNT
            && self.classified.fnv_records == AUDITED_FNV_PACK_COUNT
            && self.classified.fo3_records == AUDITED_FO3_PACK_COUNT;
        let raw_source_counts = self.raw_source_counts.unwrap_or(LegacyPackExpectedCounts {
            fnv: self.classified.fnv_records,
            fo3: self.classified.fo3_records,
        });
        let exact_raw_source_coverage = raw_source_counts == self.raw_expected;
        let exact_expected_coverage = self.observed_pack_records == self.expected.total()
            && self.classified.fnv_records == self.expected.fnv
            && self.classified.fo3_records == self.expected.fo3;
        if !exact_raw_source_coverage {
            self.increment("raw_source_count_mismatch");
        }
        if self.observed_pack_records != self.expected.total() {
            self.increment("expected_total_count_mismatch");
        }
        if self.classified.fnv_records != self.expected.fnv {
            self.increment("expected_fnv_count_mismatch");
        }
        if self.classified.fo3_records != self.expected.fo3 {
            self.increment("expected_fo3_count_mismatch");
        }
        self.provenance_errors.sort();
        self.blocked_records.sort_by(|left, right| {
            left.merged_form_key
                .cmp(&right.merged_form_key)
                .then_with(|| left.source_form_key.cmp(&right.source_form_key))
        });
        LegacyPackPreflightReport {
            raw_expected: self.raw_expected,
            raw_source_counts,
            exact_raw_source_coverage,
            expected: self.expected,
            observed_pack_records: self.observed_pack_records,
            classified: self.classified,
            exact_expected_coverage,
            provenance_errors: self.provenance_errors,
            blocker_counts: self.blocker_counts,
            blocked_records: self.blocked_records,
            explicitly_excluded: self.explicitly_excluded,
            explicitly_excluded_records: self.explicitly_excluded_records,
        }
    }

    fn take_origin(
        &mut self,
        form_key: FormKey,
        interner: &StringInterner,
    ) -> Option<ParsedOrigin> {
        let plugin = interner.resolve(form_key.plugin)?.to_ascii_lowercase();
        if let Some(origin) = self.explicit_origins.remove(&(plugin, form_key.local)) {
            return Some(origin);
        }
        let direct = self.direct_origin.as_ref()?;
        let source_form_key = rendered_form_key(form_key, interner);
        Some(ParsedOrigin {
            family: direct.family,
            row: LegacyPackOriginRow {
                merged_form_key: source_form_key.clone(),
                source_game: match direct.family {
                    LegacyPackSourceFamily::Fnv => "fnv",
                    LegacyPackSourceFamily::Fo3 => "fo3",
                    LegacyPackSourceFamily::Fo76 => "fo76",
                    LegacyPackSourceFamily::Fo4 => "fo4",
                }
                .to_string(),
                source_plugin: direct.source_plugin.clone(),
                source_form_key,
            },
        })
    }

    fn increment(&mut self, code: &str) {
        *self.blocker_counts.entry(code.to_string()).or_default() += 1;
    }
}

fn source_family(source_game: &str) -> Option<LegacyPackSourceFamily> {
    match source_game.trim().to_ascii_lowercase().as_str() {
        "fnv" => Some(LegacyPackSourceFamily::Fnv),
        "fo3" => Some(LegacyPackSourceFamily::Fo3),
        _ => None,
    }
}

fn normalized_form_key(value: &str) -> Option<(String, u32)> {
    let (local, plugin) = value.trim().split_once('@')?;
    let local = local.trim().strip_prefix("0x").unwrap_or(local.trim());
    let local = u32::from_str_radix(local, 16).ok()?;
    let plugin = plugin.trim();
    (!plugin.is_empty()).then(|| (plugin.to_ascii_lowercase(), local))
}

fn rendered_form_key(form_key: FormKey, interner: &StringInterner) -> String {
    let plugin = interner.resolve(form_key.plugin).unwrap_or("<unresolved>");
    format!("{:06X}@{plugin}", form_key.local)
}

fn resolved_editor_id(record: &Record, interner: &StringInterner) -> Option<String> {
    record
        .eid
        .and_then(|editor_id| interner.resolve(editor_id))
        .map(str::to_owned)
}

fn lowering_blocker_code(blocker: &LegacyPackLoweringBlocker) -> &'static str {
    match blocker {
        LegacyPackLoweringBlocker::NoVerifiedFo4ProcedureBlueprint { .. } => {
            "no_verified_fo4_procedure_blueprint"
        }
        LegacyPackLoweringBlocker::LegacyConditionsRequireSemanticLowering => {
            "legacy_conditions_require_semantic_lowering"
        }
        LegacyPackLoweringBlocker::LegacyEventScriptsRequirePort => {
            "legacy_event_scripts_require_port"
        }
        LegacyPackLoweringBlocker::EncodedReferencesRequireMapper => {
            "encoded_references_require_mapper"
        }
        LegacyPackLoweringBlocker::ScriptAccountingMismatch { .. } => "script_accounting_mismatch",
    }
}

fn rejection_code(rejection: &LegacyPackRejectionReason) -> &'static str {
    match rejection {
        LegacyPackRejectionReason::UnresolvedRecordIdentity { .. } => "unresolved_record_identity",
        LegacyPackRejectionReason::MissingRequiredSubrecord { .. } => "missing_required_subrecord",
        LegacyPackRejectionReason::DuplicateSubrecord { .. } => "duplicate_subrecord",
        LegacyPackRejectionReason::MalformedSubrecord { .. } => "malformed_subrecord",
        LegacyPackRejectionReason::UnknownPackageType { .. } => "unknown_package_type",
        LegacyPackRejectionReason::UnknownUnionType { .. } => "unknown_union_type",
        LegacyPackRejectionReason::DuplicateConditionCompanion { .. } => {
            "duplicate_condition_companion"
        }
        LegacyPackRejectionReason::OrphanConditionCompanion { .. } => "orphan_condition_companion",
        LegacyPackRejectionReason::MalformedScriptBlock { .. } => "malformed_script_block",
        LegacyPackRejectionReason::OrphanScriptSubrecord { .. } => "orphan_script_subrecord",
        LegacyPackRejectionReason::UnknownSubrecord { .. } => "unknown_subrecord",
    }
}

#[cfg(test)]
mod tests {
    use smallvec::SmallVec;

    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue};

    use super::*;

    fn pack(interner: &StringInterner, local: u32) -> Record {
        let mut record = Record::new(
            SigCode(*b"PACK"),
            FormKey {
                plugin: interner.intern("Merged.esm"),
                local,
            },
        );
        record.eid = Some(interner.intern("FixturePack"));
        let mut pkdt = [0_u8; 12];
        pkdt[4] = LegacyPackType::Travel.code();
        record.fields.extend([
            FieldEntry {
                sig: SubrecordSig(*b"PKDT"),
                value: FieldValue::Bytes(SmallVec::from_slice(&pkdt)),
            },
            FieldEntry {
                sig: SubrecordSig(*b"PSDT"),
                value: FieldValue::Bytes(SmallVec::from_slice(&[0_u8; 8])),
            },
        ]);
        record
    }

    fn origin(merged_local: u32, source_local: u32) -> LegacyPackOriginRow {
        LegacyPackOriginRow {
            merged_form_key: format!("{merged_local:06X}@Merged.esm"),
            source_game: "fnv".to_string(),
            source_plugin: "FalloutNV.esm".to_string(),
            source_form_key: format!("{source_local:08X}@FalloutNV.esm"),
        }
    }

    #[test]
    fn origin_keys_are_case_insensitive_and_require_plugin_identity() {
        assert_eq!(
            normalized_form_key("000800@FNV_FO3_Merged.esm"),
            Some(("fnv_fo3_merged.esm".to_string(), 0x800))
        );
        assert_eq!(normalized_form_key("000800"), None);
        assert_eq!(normalized_form_key("not-hex@Merged.esm"), None);
    }

    #[test]
    fn audited_expected_counts_are_explicit() {
        assert_eq!(LegacyPackExpectedCounts::audited_merged().total(), 9_455);
        assert_eq!(
            LegacyPackExpectedCounts::audited_for(LegacyPackSourceFamily::Fnv),
            LegacyPackExpectedCounts { fnv: 4_888, fo3: 0 }
        );
    }

    #[test]
    fn explicit_provenance_rejects_duplicate_missing_and_stale_rows() {
        let interner = StringInterner::new();
        let origins = vec![
            origin(0x800, 0x100),
            origin(0x800, 0x101),
            origin(0x900, 0x200),
        ];
        let mut accumulator = LegacyPackPreflightAccumulator::new(
            &origins,
            LegacyPackExpectedCounts { fnv: 1, fo3: 0 },
            Some(LegacyPackExpectedCounts { fnv: 1, fo3: 0 }),
            LegacyPackExpectedCounts { fnv: 1, fo3: 0 },
            None,
            true,
            false,
        );
        accumulator.observe_decoded(&pack(&interner, 0x801), &interner);
        let report = accumulator.finish();

        assert!(report.is_blocked());
        assert_eq!(report.blocker_counts.get("missing_provenance"), Some(&1));
        assert_eq!(report.blocker_counts.get("stale_provenance"), Some(&2));
        assert!(
            report
                .provenance_errors
                .iter()
                .any(|error| error.starts_with("duplicate_merged_form_key:"))
        );
        assert_eq!(
            report
                .provenance_errors
                .iter()
                .filter(|error| error.starts_with("stale_provenance:"))
                .count(),
            2
        );
    }

    #[test]
    fn explicit_pack_exclusion_waives_only_semantic_lowering() {
        let interner = StringInterner::new();
        let origins = vec![origin(0x800, 0x100)];
        let counts = LegacyPackExpectedCounts { fnv: 1, fo3: 0 };
        let mut default_gate = LegacyPackPreflightAccumulator::new(
            &origins,
            counts,
            Some(counts),
            counts,
            None,
            true,
            false,
        );
        default_gate.observe_decoded(&pack(&interner, 0x800), &interner);
        assert!(default_gate.finish().is_blocked());

        let mut excluded_gate = LegacyPackPreflightAccumulator::new(
            &origins,
            counts,
            Some(counts),
            counts,
            None,
            true,
            true,
        );
        excluded_gate.observe_decoded(&pack(&interner, 0x800), &interner);
        let report = excluded_gate.finish();

        assert!(!report.is_blocked());
        assert!(report.exact_raw_source_coverage);
        assert!(report.exact_expected_coverage);
        assert!(report.explicitly_excluded);
        assert_eq!(report.explicitly_excluded_records, 1);
        assert!(report.blocked_records.is_empty());
        assert_eq!(report.classified.accepted_records, 0);
        assert_eq!(report.classified.rejected_records, 0);
    }

    #[test]
    fn explicit_pack_exclusion_does_not_waive_raw_census_mismatch() {
        let interner = StringInterner::new();
        let origins = vec![origin(0x800, 0x100)];
        let final_counts = LegacyPackExpectedCounts { fnv: 1, fo3: 0 };
        let mut accumulator = LegacyPackPreflightAccumulator::new(
            &origins,
            LegacyPackExpectedCounts { fnv: 2, fo3: 0 },
            Some(final_counts),
            final_counts,
            None,
            true,
            true,
        );
        accumulator.observe_decoded(&pack(&interner, 0x800), &interner);
        let report = accumulator.finish();

        assert!(report.is_blocked());
        assert!(!report.exact_raw_source_coverage);
        assert_eq!(report.blocker_counts["raw_source_count_mismatch"], 1);
    }
}
