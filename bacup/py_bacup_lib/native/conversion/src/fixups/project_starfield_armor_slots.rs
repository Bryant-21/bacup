use std::collections::HashMap;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::session::PluginSession;
use crate::translator::pair_hooks::starfield_fo4::armor;

pub struct ProjectStarfieldArmorSlotsFixup;

impl Fixup for ProjectStarfieldArmorSlotsFixup {
    fn name(&self) -> &'static str {
        "project_starfield_armor_slots"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        config
            .source_schema
            .as_ref()
            .and_then(|schema| schema.record_def("ARMA"))
            .is_some_and(|record| record.subrecords.iter().any(|field| field.id == "BO64"))
            && config
                .target_schema
                .as_ref()
                .and_then(|schema| schema.record_def("ARMA"))
                .is_some_and(|record| record.subrecords.iter().any(|field| field.id == "BOD2"))
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema".into()))?;
        let mut report = FixupReport::empty();
        let mut masks = HashMap::new();
        let arma = SigCode::from_str("ARMA").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let armo = SigCode::from_str("ARMO").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        for fk in session
            .form_keys_of_sig(arma, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
        {
            let record = session
                .record_decoded(&fk, schema, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            masks.insert(fk, armor::biped_mask(&record));
        }
        let mut changed = Vec::new();
        for fk in session
            .form_keys_of_sig(armo, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
        {
            let mut record = session
                .record_decoded(&fk, schema, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            if armor::union_addon_slots(&mut record, &masks) {
                changed.push(record);
            }
        }
        let expected = changed.len();
        if expected > 0 {
            let replaced = session
                .replace_records_contents(changed, schema, mapper.interner)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            if replaced != expected {
                return Err(FixupError::HandleError(format!(
                    "Starfield armor slot union replaced {replaced} of {expected} records"
                )));
            }
            report.records_changed = u32::try_from(replaced)
                .map_err(|_| FixupError::Other("armor count exceeds u32".into()))?;
        }
        Ok(report)
    }
}
