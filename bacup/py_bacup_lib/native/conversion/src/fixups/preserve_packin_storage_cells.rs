//! Fixup: preserve storage CELL records referenced by PKIN records.
//!
//! FO76 pack-ins can point their `CNAM`/`Cell` field at anonymous storage cells.
//! Top-level `CELL` records are skipped by the normal FO76 -> FO4 translator
//! because exterior/worldspace cells need special group placement. These storage
//! cells are a narrow exception: without a target `CELL` definition, the
//! Creation Kit logs "No storage cell defined" for each pack-in and can crash
//! while loading the master.

use rustc_hash::FxHashSet;

use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::source_read::form_key_to_read_str;
use crate::sym::Sym;

pub struct PreservePackinStorageCellsFixup;

impl Fixup for PreservePackinStorageCellsFixup {
    fn name(&self) -> &'static str {
        "preserve_packin_storage_cells"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, ctx: &FixupContext) -> bool {
        ctx.source_handle_id != 0
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        session.source_id().is_some()
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let target_schema = session
            .schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let source_schema = session
            .source_schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let source_plugin_name = match session.source_slot_opt() {
            Some(slot) => slot.parsed.plugin_name.clone(),
            None => return Ok(report),
        };
        let output_plugin_name = session.target_slot().parsed.plugin_name.clone();
        let source_plugin_sym = mapper.interner.intern(&source_plugin_name);
        let output_plugin_sym = mapper.interner.intern(&output_plugin_name);
        let pkin_sig =
            SigCode::from_str("PKIN").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let cell_sig =
            SigCode::from_str("CELL").map_err(|e| FixupError::SchemaError(e.to_string()))?;

        let packin_fks = session
            .form_keys_of_sig(pkin_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if packin_fks.is_empty() {
            return Ok(report);
        }

        let target_handle_id = session.target_id();
        let mut target_cells_to_add = Vec::new();
        let mut mappings_to_add = Vec::new();
        let mut seen_cells = FxHashSet::default();
        let mut queued_target_cells = FxHashSet::default();
        for packin_fk in packin_fks {
            let packin =
                match session.record_decoded(&packin_fk, target_schema.as_ref(), mapper.interner) {
                    Ok(record) => record,
                    Err(e) => {
                        let warning = mapper
                            .interner
                            .intern(&format!("preserve_packin_storage_cells:pkin_read_err:{e}"));
                        report.warnings.push(warning);
                        continue;
                    }
                };

            let Some(cell_ref) = packin_storage_cell_ref(&packin) else {
                continue;
            };
            let Some(source_cell_fk) =
                source_cell_fk(cell_ref, source_plugin_sym, output_plugin_sym)
            else {
                continue;
            };
            if !seen_cells.insert(source_cell_fk) {
                continue;
            }

            let target_cell_fk = mapper.lookup(source_cell_fk).unwrap_or(FormKey {
                local: source_cell_fk.local,
                plugin: output_plugin_sym,
            });
            let target_cell_key = form_key_to_read_str(&target_cell_fk, mapper.interner);
            if target_cell_key.is_empty() {
                continue;
            }

            let existing_sig = session
                .record_signature_in_handle(target_handle_id, &target_cell_key)
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            if let Some(sig) = existing_sig {
                if sig == "CELL" {
                    mapper.add_mapping(source_cell_fk, target_cell_fk);
                } else {
                    let warning = mapper.interner.intern(&format!(
                        "preserve_packin_storage_cells:target_conflict:{}:{}",
                        target_cell_key, sig
                    ));
                    report.warnings.push(warning);
                }
                continue;
            }
            if queued_target_cells.contains(&target_cell_fk) {
                mappings_to_add.push((source_cell_fk, target_cell_fk));
                continue;
            }

            let source_cell = match session.source_record_decoded(
                &source_cell_fk,
                source_schema.as_ref(),
                mapper.interner,
            ) {
                Ok(record) => record,
                Err(e) => {
                    let warning = mapper
                        .interner
                        .intern(&format!("preserve_packin_storage_cells:cell_read_err:{e}"));
                    report.warnings.push(warning);
                    continue;
                }
            };
            if source_cell.sig != cell_sig {
                continue;
            }

            let target_cell = build_minimal_storage_cell(&source_cell, target_cell_fk, cell_sig)?;
            queued_target_cells.insert(target_cell_fk);
            target_cells_to_add.push(target_cell);
            mappings_to_add.push((source_cell_fk, target_cell_fk));
        }

        let records_added = session
            .add_records(target_cells_to_add, target_schema.as_ref(), mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        report.records_added = records_added.try_into().unwrap_or(u32::MAX);
        for (source_cell_fk, target_cell_fk) in mappings_to_add {
            mapper.add_mapping(source_cell_fk, target_cell_fk);
        }

        Ok(report)
    }
}

fn packin_storage_cell_ref(record: &Record) -> Option<FormKey> {
    let cnam_sig = SubrecordSig::from_str("CNAM").ok()?;
    record
        .fields
        .iter()
        .find(|entry| entry.sig == cnam_sig)
        .and_then(|entry| match &entry.value {
            FieldValue::FormKey(fk) if fk.local != 0 => Some(*fk),
            _ => None,
        })
}

fn source_cell_fk(
    cell_ref: FormKey,
    source_plugin_sym: Sym,
    output_plugin_sym: Sym,
) -> Option<FormKey> {
    if cell_ref.local == 0 {
        return None;
    }
    if cell_ref.plugin == source_plugin_sym || cell_ref.plugin == output_plugin_sym {
        return Some(FormKey {
            local: cell_ref.local,
            plugin: source_plugin_sym,
        });
    }
    None
}

fn build_minimal_storage_cell(
    source_cell: &Record,
    target_fk: FormKey,
    cell_sig: SigCode,
) -> Result<Record, FixupError> {
    let data_sig =
        SubrecordSig::from_str("DATA").map_err(|e| FixupError::SchemaError(e.to_string()))?;
    let mut target_cell = Record::new(cell_sig, target_fk);
    for entry in source_cell
        .fields
        .iter()
        .filter(|entry| entry.sig == data_sig)
    {
        target_cell.fields.push(entry.clone());
    }
    if target_cell.fields.is_empty() {
        target_cell.fields.push(FieldEntry {
            sig: data_sig,
            value: FieldValue::Uint(0),
        });
    }
    Ok(target_cell)
}
