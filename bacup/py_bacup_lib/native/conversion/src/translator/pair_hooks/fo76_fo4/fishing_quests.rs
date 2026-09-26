use super::*;
use crate::sym::StringInterner;

impl Fo76Fo4Hook {
    pub(super) fn adapt_fishing_quest_aliases(interner: &StringInterner, record: &mut Record) {
        if record.sig.0 != *b"QUST"
            || !interner
                .resolve(record.form_key.plugin)
                .is_some_and(|name| name.eq_ignore_ascii_case(FO76_MASTER_NAME))
        {
            return;
        }
        let contracts = [
            (0x7BD0A8, "fishing_mq01_changelocation"),
            (0x7ACB4C, "fishing_mq01_casting"),
            (0x7B95E1, "fishing_bigfish"),
            (0x7B7338, "fishing_fishermansrest_dialogue"),
            (0x7D18D3, "fishing_mq01_casting_radio"),
        ];
        let eid = qust_eid_lower(interner, record);
        if !contracts
            .iter()
            .any(|(id, name)| record.form_key.local == *id && eid.as_deref() == Some(*name))
        {
            return;
        }
        let starter = record.form_key.local == 0x7BD0A8;
        let mut alias = None;
        record.fields.retain_mut(|field| {
            match &field.sig.0 {
                b"ALST" | b"ALLS" => alias = field_value_to_u32(&field.value),
                b"ALED" => alias = None,
                // FO76 shares these actors between per-player quests. FO4's global
                // alias reservation must permit the same ambient/story/daily cast.
                b"FNAM" if alias.is_some() => {
                    if let Some(flags) = field_value_to_u32(&field.value) {
                        field.value = FieldValue::Uint(u64::from(flags | 0x200));
                    }
                }
                b"VTCK" if record.form_key.local == 0x7B95E1 && alias == Some(3) => {
                    if matches!(field.value, FieldValue::FormKey(fk) if fk.plugin == record.form_key.plugin && fk.local == 0x7ACD7A) {
                        field.value = FieldValue::FormKey(FormKey { plugin: record.form_key.plugin, local: 0x7ACD7E });
                    }
                }
                b"ALFD" if starter && alias == Some(0) => return false,
                b"ALFE" if starter && alias == Some(0) => {
                    field.sig = SubrecordSig(*b"ALFR");
                    field.value = FieldValue::FormKey(FormKey {
                        plugin: interner.intern(FO4_MASTER_NAME), local: FO4_PLAYER_REF_FORM_ID,
                    });
                }
                _ => {}
            }
            true
        });
    }
}
