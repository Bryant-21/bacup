//! Fill the Vault 94 exterior terminal's `<Token.Name=StatusBar>` with fixed text.
//!
//! FO76 `V94TerminalScript` filled the token with one of the VaultSystem status
//! messages, chosen from raid state. FO4 runs a Terminal script only on
//! `OnMenuItemRun`, after the body text is already on screen, and the raids are
//! retired, so the token would print raw. The body is rewritten once, in every
//! language, with the idle (no mission) message `VaultSystemTRMsg_VaultStatus_NormalBar`.

use esp_authoring_core::plugin_runtime::ParsedSubrecord;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::session::PluginSession;

const V94_EXTERIOR_TERMINAL: u32 = 0x14_FA8D;
const VAULT_STATUS_NORMAL_BAR: u32 = 0x2F_B4AD;
const STATUS_TOKEN: &str = "<Token.Name=StatusBar>";
const TES4_FLAG_LOCALIZED: u32 = 0x80;

pub struct FillFo76VaultStatusTokenFixup;

impl Fixup for FillFo76VaultStatusTokenFixup {
    fn name(&self) -> &'static str {
        "fill_fo76_vault_status_token"
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
        let interner = mapper.interner;
        if session.target_slot().parsed.header.flags & TES4_FLAG_LOCALIZED == 0 {
            report.message =
                Some(interner.intern("fo76_vault_status_token:skipped=target_not_localized"));
            return Ok(report);
        }
        let own = (session.target_masters().len() as u32) << 24;
        let target_id = session.target_id();
        let scan = session.handle_raw_scan(target_id).map_err(handle_error)?;
        let body_ids = scan
            .with_record_subrecords(own | V94_EXTERIOR_TERMINAL, |subrecords| {
                string_ids(subrecords, "BTXT")
            })
            .unwrap_or_default();
        let status_id = scan
            .with_record_subrecords(own | VAULT_STATUS_NORMAL_BAR, |subrecords| {
                string_ids(subrecords, "FULL").into_iter().next()
            })
            .flatten();
        drop(scan);
        let Some(status_id) = status_id else {
            report.message =
                Some(interner.intern("fo76_vault_status_token:skipped=no_status_message"));
            return Ok(report);
        };

        let strings = session.target_slot_mut().strings_mut();
        strings.materialize_all();
        let mut rewritten = 0u32;
        for language in strings.language_codes() {
            let Some(status) = strings.resolve(&language, status_id) else {
                continue;
            };
            for body_id in &body_ids {
                let Some(body) = strings.resolve(&language, *body_id) else {
                    continue;
                };
                if !body.contains(STATUS_TOKEN) {
                    continue;
                }
                strings
                    .by_language
                    .entry(language.clone())
                    .or_default()
                    .insert(*body_id, body.replace(STATUS_TOKEN, &status));
                rewritten += 1;
            }
        }
        report.message = Some(interner.intern(&format!(
            "fo76_vault_status_token:bodies={};strings_rewritten={rewritten}",
            body_ids.len()
        )));
        Ok(report)
    }
}

fn string_ids(subrecords: &[ParsedSubrecord], signature: &str) -> Vec<u32> {
    subrecords
        .iter()
        .filter(|subrecord| subrecord.signature.as_str() == signature)
        .filter_map(|subrecord| {
            let id = u32::from_le_bytes(subrecord.data.as_ref().try_into().ok()?);
            (id != 0).then_some(id)
        })
        .collect()
}

fn handle_error(error: impl std::fmt::Display) -> FixupError {
    FixupError::HandleError(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record};
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::plugin_handle_new_native;

    const PLUGIN: &str = "SeventySix.esm";
    const BODY: u32 = 0x3001;
    const STATUS: u32 = 0x3002;

    fn record(
        sig: &[u8; 4],
        local: u32,
        text_sig: &[u8; 4],
        id: u32,
        interner: &StringInterner,
    ) -> Record {
        let mut record = Record::new(
            SigCode(*sig),
            FormKey {
                plugin: interner.intern(PLUGIN),
                local,
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*text_sig),
            value: FieldValue::Bytes(id.to_le_bytes().to_vec().into()),
        });
        record
    }

    #[test]
    fn exterior_terminal_status_token_becomes_normal_bar_in_every_language() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native(PLUGIN, Some("fo76")).unwrap();
        let target = plugin_handle_new_native(PLUGIN, Some("fo4")).unwrap();
        let mut session = open_session(target, Some(source)).unwrap();
        session
            .target_slot_mut()
            .parsed
            .header
            .masters
            .push("Fallout4.esm".into());
        session.target_slot_mut().parsed.header.flags |= TES4_FLAG_LOCALIZED;
        {
            let strings = session.target_slot_mut().strings_mut();
            strings.default_language = "en".into();
            for (language, body, status) in [
                ("en", "|Vault Status: <Token.Name=StatusBar>\n", "NORMAL |"),
                (
                    "de",
                    "|Vault-Status: <Token.Name=StatusBar>\n",
                    "NORMAL (de) |",
                ),
            ] {
                let table = strings.by_language.entry(language.into()).or_default();
                table.insert(BODY, body.into());
                table.insert(STATUS, status.into());
            }
            strings.table_types.insert(BODY, "strings".into());
            strings.table_types.insert(STATUS, "strings".into());
        }
        let schema = session.schema().unwrap();
        for record in [
            record(b"TERM", V94_EXTERIOR_TERMINAL, b"BTXT", BODY, &interner),
            record(b"MESG", VAULT_STATUS_NORMAL_BAR, b"FULL", STATUS, &interner),
        ] {
            session.add_record(record, &schema, &interner).unwrap();
        }
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: PLUGIN.into(),
                source_plugin_name: PLUGIN.into(),
                ..MapperOptions::default()
            },
            &interner,
        );

        let fixup = FillFo76VaultStatusTokenFixup;
        assert!(fixup.applies_to_session(&session, &FixupConfig::default()));
        fixup
            .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
            .unwrap();

        let strings = session.target_slot().strings_ref();
        assert_eq!(
            strings.resolve("en", BODY).unwrap(),
            "|Vault Status: NORMAL |\n"
        );
        assert_eq!(
            strings.resolve("de", BODY).unwrap(),
            "|Vault-Status: NORMAL (de) |\n"
        );
        assert_eq!(strings.resolve("en", STATUS).unwrap(), "NORMAL |");
    }
}
