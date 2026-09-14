use crate::fixups::synthesize_legacy_music::{field, sig, subrecord};
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::FormKey;
use crate::record::FieldValue;
use crate::session::PluginSession;

/// FO3-era music type the legacy engine played for exteriors by default.
/// `synthesize_legacy_music` has already populated it with FO4 tracks.
const FALLBACK_MUSIC_TYPE: &str = "DefaultExplore";

/// Give converted worldspaces a fallback explore music type.
///
/// Legacy exteriors name their music per-cell (`XCMO`), which translates
/// directly, but FO3 and FNV also leaned on engine defaults and on placed
/// `ALOC` media-location controllers that have no FO4 equivalent. FO4 plays
/// music only where a CELL, WRLD, or LCTN names a MUSC, and every vanilla FO4
/// worldspace sets `ZNAM` — so a converted worldspace with no `ZNAM` is silent
/// in any cell that did not carry its own music type.
pub struct AssignLegacyWorldspaceMusicFixup;

impl Fixup for AssignLegacyWorldspaceMusicFixup {
    fn name(&self) -> &'static str {
        "assign_legacy_worldspace_music"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        let source_game = session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref());
        let target_game = session.target_slot().parsed.game.as_deref();
        matches!(source_game, Some("fnv" | "fo3")) && target_game == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let target_schema = session
            .schema()
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let znam_sig = subrecord("ZNAM")?;
        let mut report = FixupReport::empty();

        let form_keys = session
            .form_keys_of_sig(sig("WRLD")?, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let mut silent = Vec::new();
        for form_key in form_keys {
            let record = session
                .record_decoded(&form_key, target_schema.as_ref(), mapper.interner)
                .map_err(|error| FixupError::HandleError(error.to_string()))?;
            if !record.fields.iter().any(|entry| entry.sig == znam_sig) {
                silent.push(record);
            }
        }
        if silent.is_empty() {
            return Ok(report);
        }

        let Some(music_type) = mapped_music_type(session, mapper, FALLBACK_MUSIC_TYPE)? else {
            report.warnings.push(mapper.interner.intern(&format!(
                "assign_legacy_worldspace_music:no_explore_music:{}",
                silent.len()
            )));
            return Ok(report);
        };

        for record in &mut silent {
            record
                .fields
                .push(field("ZNAM", FieldValue::FormKey(music_type))?);
        }
        report.records_changed = session
            .replace_records_contents(silent, target_schema.as_ref(), mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?
            .try_into()
            .unwrap_or(u32::MAX);
        Ok(report)
    }
}

/// Target FormKey of the converted source MUSC named `editor_id`.
fn mapped_music_type(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    editor_id: &str,
) -> Result<Option<FormKey>, FixupError> {
    let source_schema = session
        .source_schema()
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    let form_keys = session
        .source_form_keys_of_sig(sig("MUSC")?, mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    for form_key in form_keys {
        let record = session
            .source_record_decoded(&form_key, source_schema.as_ref(), mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        if record
            .eid
            .and_then(|eid| mapper.interner.resolve(eid))
            .is_some_and(|found| found.eq_ignore_ascii_case(editor_id))
        {
            return Ok(mapper.lookup(form_key));
        }
    }
    Ok(None)
}
