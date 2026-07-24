use super::super::fnv_magic_effects::{
    LegacyMagicFamily, MagicEffectsNormalizeReport, MagicReferenceOutcome,
    normalize_legacy_magic_effects,
};
use super::super::fnv_mgef::{
    LegacyMgefFamily, MgefNormalizeReport, MgefReferenceOutcome, normalize_legacy_mgef_data,
};
use super::super::fnv_perk::{
    LegacyPerkFamily, PerkNormalizeReport, PerkReferenceOutcome, normalize_legacy_perk,
};
use super::super::fnv_wrld::{
    WrldNormalizationError, WrldNormalizationReport, WrldReferenceState, WrldSourceFamily,
    normalize_wrld_for_fo4,
};
use crate::formkey_mapper::FormKeyMapper;
use crate::record::Record;
use crate::translator::Game;
#[derive(Debug)]
pub(crate) enum LegacySerialNormalizeReport {
    Mgef(MgefNormalizeReport),
    Effects(MagicEffectsNormalizeReport),
    Wrld(WrldNormalizationReport),
    Perk(PerkNormalizeReport),
    PerkAlreadyNormalized,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LegacySerialDiagnostic {
    pub warning: bool,
    pub message: String,
}

#[derive(Debug, Default)]
pub(crate) struct LegacySerialNormalizationState {
    normalized_perks: rustc_hash::FxHashSet<crate::ids::FormKey>,
}

impl LegacySerialNormalizationState {
    pub(crate) fn clear(&mut self) {
        self.normalized_perks.clear();
    }

    fn mark_perk(&mut self, source_fk: crate::ids::FormKey) -> bool {
        self.normalized_perks.insert(source_fk)
    }
}

impl LegacySerialNormalizeReport {
    pub(crate) fn register_target_identities(&self, mapper: &mut FormKeyMapper<'_>) {
        match self {
            Self::Mgef(report) => {
                for decision in &report.references {
                    if let MgefReferenceOutcome::ResolvedTarget { target, .. } = decision.outcome {
                        mapper.add_mapping(target, target);
                    }
                }
            }
            Self::Effects(report) => {
                for decision in &report.references {
                    if let MagicReferenceOutcome::MappedTyped { target, .. } = decision.outcome {
                        mapper.add_mapping(target, target);
                    }
                }
            }
            Self::Perk(report) => {
                for decision in &report.references {
                    if let PerkReferenceOutcome::MappedTyped { target, .. } = decision.outcome {
                        mapper.add_mapping(target, target);
                    }
                }
            }
            Self::Wrld(_) | Self::PerkAlreadyNormalized => {}
        }
    }

    pub(crate) fn diagnostics(&self, record: &Record) -> Vec<LegacySerialDiagnostic> {
        let prefix = format!(
            "legacy_serial:{}:{:06X}",
            record.sig.as_str(),
            record.form_key.local
        );
        match self {
            Self::Mgef(report) => {
                let mut diagnostics =
                    Vec::with_capacity(report.references.len() + report.enums.len() + 1);
                diagnostics.push(LegacySerialDiagnostic {
                    warning: report.unsupported_rows != 0,
                    message: format!(
                        "{prefix}:summary:converted={};preserved={};unsupported={}",
                        report.converted_rows,
                        report.preserved_target_rows,
                        report.unsupported_rows
                    ),
                });
                diagnostics.extend(report.references.iter().map(|decision| {
                    let warning = matches!(
                        decision.outcome,
                        MgefReferenceOutcome::DeferredNull { .. }
                            | MgefReferenceOutcome::UnsupportedValue { .. }
                            | MgefReferenceOutcome::MissingTarget { .. }
                    );
                    LegacySerialDiagnostic {
                        warning,
                        message: format!(
                            "{prefix}:reference:{}:{:?}",
                            decision.field, decision.outcome
                        ),
                    }
                }));
                diagnostics.extend(report.enums.iter().map(|decision| LegacySerialDiagnostic {
                    warning: decision.used_default,
                    message: format!(
                        "{prefix}:enum:{}:source={};target={};default={}",
                        decision.field, decision.source, decision.target, decision.used_default
                    ),
                }));
                diagnostics
            }
            Self::Effects(report) => {
                let mut diagnostics =
                    Vec::with_capacity(report.references.len() + report.enums.len() + 1);
                diagnostics.push(LegacySerialDiagnostic {
                    warning: report.dropped_effects != 0
                        || report.dropped_conditions != 0
                        || report.dropped_metadata_rows != 0,
                    message: format!(
                        "{prefix}:summary:effects={}/{}/{};conditions={}/{}/{};metadata={}/{}/{};orphan_strings={}",
                        report.converted_effects,
                        report.preserved_target_effects,
                        report.dropped_effects,
                        report.converted_conditions,
                        report.preserved_target_conditions,
                        report.dropped_conditions,
                        report.converted_metadata_rows,
                        report.preserved_target_metadata_rows,
                        report.dropped_metadata_rows,
                        report.orphan_condition_strings_dropped
                    ),
                });
                diagnostics.extend(report.references.iter().map(|decision| {
                    let warning = matches!(
                        decision.outcome,
                        MagicReferenceOutcome::UnmappedRaw { .. }
                            | MagicReferenceOutcome::UnmappedTyped { .. }
                            | MagicReferenceOutcome::UnsupportedValue
                    );
                    LegacySerialDiagnostic {
                        warning,
                        message: format!(
                            "{prefix}:reference:effect={:?}:{}:{:?}",
                            decision.effect_index, decision.field, decision.outcome
                        ),
                    }
                }));
                diagnostics.extend(report.enums.iter().map(|decision| LegacySerialDiagnostic {
                    warning: decision.used_default || decision.dropped_bits != 0,
                    message: format!(
                        "{prefix}:enum:{}:source={};target={};dropped_bits={:#X};default={}",
                        decision.field,
                        decision.source,
                        decision.target,
                        decision.dropped_bits,
                        decision.used_default
                    ),
                }));
                diagnostics
            }
            Self::Wrld(report) => {
                let mut diagnostics =
                    Vec::with_capacity(report.data_changes.len() + report.references.len() + 1);
                diagnostics.push(LegacySerialDiagnostic {
                    warning: report.synthesized_data_default || report.dropped_inam_fields != 0,
                    message: format!(
                        "{prefix}:wrld_summary:applied={};synthesized_data={};dropped_inam={}",
                        report.applied, report.synthesized_data_default, report.dropped_inam_fields
                    ),
                });
                diagnostics.extend(report.data_changes.iter().map(|change| LegacySerialDiagnostic {
                    warning: change.dropped_source_flags != 0,
                    message: format!(
                        "{prefix}:wrld_data:index={};source={:#04X};target={:#04X};dropped={:#04X}",
                        change.field_index,
                        change.source_flags,
                        change.target_flags,
                        change.dropped_source_flags
                    ),
                }));
                diagnostics.extend(report.references.iter().map(|reference| {
                    LegacySerialDiagnostic {
                        warning: reference.state == WrldReferenceState::PreservedInvalid,
                        message: format!(
                            "{prefix}:wrld_reference:{}:{:?}",
                            String::from_utf8_lossy(&reference.sig),
                            reference.state
                        ),
                    }
                }));
                diagnostics
            }
            Self::Perk(report) => {
                let mut diagnostics = Vec::with_capacity(
                    report.references.len() + report.enums.len() + report.drops.len() + 1,
                );
                diagnostics.push(LegacySerialDiagnostic {
                    warning: report.dropped_entries != 0
                        || report.dropped_conditions != 0
                        || report.orphan_companions_dropped != 0,
                    message: format!(
                        "{prefix}:perk_summary:entries={}/{};conditions={}/{}/{};orphan_companions={}",
                        report.converted_entries,
                        report.dropped_entries,
                        report.converted_conditions,
                        report.preserved_target_conditions,
                        report.dropped_conditions,
                        report.orphan_companions_dropped
                    ),
                });
                diagnostics.extend(report.references.iter().map(|decision| {
                    let warning = matches!(
                        decision.outcome,
                        PerkReferenceOutcome::UnmappedRaw { .. }
                            | PerkReferenceOutcome::UnmappedTyped { .. }
                            | PerkReferenceOutcome::UnsupportedValue
                    );
                    LegacySerialDiagnostic {
                        warning,
                        message: format!(
                            "{prefix}:perk_reference:entry={:?}:condition={:?}:{}:{:?}",
                            decision.entry_index,
                            decision.condition_index,
                            decision.field,
                            decision.outcome
                        ),
                    }
                }));
                diagnostics.extend(report.enums.iter().map(|decision| LegacySerialDiagnostic {
                    warning: false,
                    message: format!(
                        "{prefix}:perk_enum:entry={:?}:{}:source={};target={}",
                        decision.entry_index, decision.field, decision.source, decision.target
                    ),
                }));
                diagnostics.extend(report.drops.iter().map(|decision| LegacySerialDiagnostic {
                    warning: true,
                    message: format!(
                        "{prefix}:perk_drop:entry={:?}:condition={:?}:{:?}",
                        decision.entry_index, decision.condition_index, decision.reason
                    ),
                }));
                diagnostics
            }
            Self::PerkAlreadyNormalized => vec![LegacySerialDiagnostic {
                warning: false,
                message: format!("{prefix}:perk_already_normalized"),
            }],
        }
    }
}

pub(crate) fn normalize_legacy_serial_record_once(
    source: Game,
    target: Game,
    source_fk: crate::ids::FormKey,
    record: &mut Record,
    mapper: &mut FormKeyMapper<'_>,
    state: &mut LegacySerialNormalizationState,
) -> Option<Result<LegacySerialNormalizeReport, LegacySerialDiagnostic>> {
    if target != Game::Fo4 {
        return None;
    }
    let (mgef_family, magic_family, perk_family, wrld_family) = match source {
        Game::Fnv => (
            LegacyMgefFamily::Fnv,
            LegacyMagicFamily::Fnv,
            LegacyPerkFamily::Fnv,
            WrldSourceFamily::Fnv,
        ),
        Game::Fo3 => (
            LegacyMgefFamily::Fo3,
            LegacyMagicFamily::Fo3,
            LegacyPerkFamily::Fo3,
            WrldSourceFamily::Fo3,
        ),
        _ => return None,
    };
    match record.sig.0 {
        sig if sig == *b"MGEF" => Some(Ok(LegacySerialNormalizeReport::Mgef(
            normalize_legacy_mgef_data(record, mgef_family, mapper),
        ))),
        sig if matches!(sig, s if s == *b"ALCH" || s == *b"ENCH" || s == *b"SPEL") => {
            Some(Ok(LegacySerialNormalizeReport::Effects(
                normalize_legacy_magic_effects(record, magic_family, mapper),
            )))
        }
        sig if sig == *b"WRLD" => Some(
            normalize_wrld_for_fo4(record, wrld_family, mapper.interner)
                .map(LegacySerialNormalizeReport::Wrld)
                .map_err(|error| wrld_error_diagnostic(record, error)),
        ),
        sig if sig == *b"PERK" => {
            if !state.mark_perk(source_fk) {
                return Some(Ok(LegacySerialNormalizeReport::PerkAlreadyNormalized));
            }
            Some(Ok(LegacySerialNormalizeReport::Perk(
                normalize_legacy_perk(record, perk_family, mapper),
            )))
        }
        _ => None,
    }
}

fn wrld_error_diagnostic(record: &Record, error: WrldNormalizationError) -> LegacySerialDiagnostic {
    LegacySerialDiagnostic {
        warning: true,
        message: format!(
            "legacy_serial:WRLD:{:06X}:wrld_drop:{error:?}",
            record.form_key.local
        ),
    }
}
