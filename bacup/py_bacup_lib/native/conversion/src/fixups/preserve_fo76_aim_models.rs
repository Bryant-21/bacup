use crate::fixups::quest_script_binding::SourcePlugin;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::record::{Record, RecordFlags};
use crate::session::PluginSession;

pub struct PreserveFo76AimModelsFixup;

impl Fixup for PreserveFo76AimModelsFixup {
    fn name(&self) -> &'static str {
        "preserve_fo76_aim_models"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref())
            == Some("fo76")
            && session.target_slot().parsed.game.as_deref() == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let Some(source) = SourcePlugin::of(session, mapper.interner) else {
            return Ok(report);
        };
        let aim_sig = SigCode(*b"AMDL");
        let models = {
            let scan = session.handle_raw_scan(source.id).map_err(handle_error)?;
            source
                .own_raw_form_ids(&scan, aim_sig)
                .into_iter()
                .filter_map(|raw| {
                    let source = source.form_key(raw, mapper.interner)?;
                    let target = mapper
                        .lookup(source)
                        .filter(|target| target.plugin != mapper.output_plugin_sym())?;
                    Some((source, target))
                })
                .collect::<Vec<_>>()
        };
        let source_schema = session.source_schema().map_err(handle_error)?;
        let target_schema = session.schema().map_err(handle_error)?;
        let existing = session
            .form_keys_of_sig(aim_sig, mapper.interner)
            .map_err(handle_error)?;
        let mut replacements = Vec::new();
        for (source, target) in models {
            let source_record = session
                .source_record_decoded(&source, &source_schema, mapper.interner)
                .map_err(handle_error)?;
            if source_record.flags.contains(RecordFlags::DELETED) {
                continue;
            }
            if !source_record
                .fields
                .iter()
                .any(|field| field.sig.0 == *b"DNAM")
            {
                report.warnings.push(mapper.interner.intern(&format!(
                    "preserve_fo76_aim_models:{:06X}:missing_data",
                    source.local
                )));
                continue;
            }
            // Reused weapons and receiver mods already point here; an override
            // preserves their identity while replacing FO4's tuning with FO76's.
            let mut record = Record::new(aim_sig, target);
            record.eid = source_record.eid;
            record.fields.extend(
                source_record
                    .fields
                    .into_iter()
                    .filter(|field| matches!(field.sig.as_str(), "EDID" | "DNAM")),
            );
            if existing.contains(&target) {
                let previous = session
                    .record_decoded(&target, &target_schema, mapper.interner)
                    .map_err(handle_error)?;
                if previous.fields != record.fields {
                    replacements.push(record);
                }
            } else {
                session
                    .add_record(record, &target_schema, mapper.interner)
                    .map_err(handle_error)?;
                report.records_added += 1;
            }
        }
        if !replacements.is_empty() {
            report.records_changed = session
                .replace_records_contents(replacements, &target_schema, mapper.interner)
                .map_err(handle_error)? as u32;
        }
        Ok(report)
    }
}

fn handle_error(error: impl std::fmt::Display) -> FixupError {
    FixupError::HandleError(error.to_string())
}

#[cfg(test)]
mod tests;
