use super::*;
use crate::sym::StringInterner;
use crate::translator::pair_hooks::fnv_mgef::{null_reference, push_field};

/// FO4 ALCH ENIT flag: No Auto-Calc.
const ALCH_NO_AUTO_CALC: u32 = 0x1;

impl Fo76Fo4Hook {
    pub(super) fn lower_repair_kits(interner: &StringInterner, record: &mut Record) {
        if record.sig.0 != *b"UTIL" {
            return;
        }
        let Some(name) = record.eid.and_then(|eid| interner.resolve(eid)) else {
            return;
        };
        if name == "ATX_Utility_ScrapToStash" {
            Self::lower_scrap_to_stash(interner, record);
            return;
        }
        if !matches!(
            name,
            "ATX_Utility_RepairKit_Basic" | "Utility_RepairKit_Improved"
        ) {
            return;
        }

        // FO4 has no UTIL form. Tales consumes these inventory items and applies the
        // source UITE Set-health targets (1.0 / 1.5) to the selected equipment stack.
        record.sig = crate::ids::SigCode(*b"MISC");
        record.fields.retain(|field| {
            matches!(
                field.sig.as_str(),
                "EDID"
                    | "OBND"
                    | "PTRN"
                    | "FULL"
                    | "MODL"
                    | "MODT"
                    | "MODS"
                    | "MODC"
                    | "KSIZ"
                    | "KWDA"
                    | "DATA"
                    | "YNAM"
                    | "ZNAM"
            )
        });
    }

    /// Tales reacts to the player consuming this item from the Pip-Boy Aid tab, which
    /// FO4 only offers for ALCH forms.
    fn lower_scrap_to_stash(interner: &StringInterner, record: &mut Record) {
        let data = record
            .fields
            .iter()
            .find_map(|field| match (&field.sig.0, &field.value) {
                (b"DATA", FieldValue::Bytes(bytes)) if bytes.len() >= 8 => Some(bytes.to_vec()),
                _ => None,
            });
        let sound_consume = record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"UIUS" && matches!(field.value, FieldValue::FormKey(_)))
            .map_or_else(null_reference, |field| field.value.clone());
        let (value, weight) = data.map_or(([0; 4], [0; 4]), |bytes| {
            (
                bytes[0..4].try_into().unwrap(),
                bytes[4..8].try_into().unwrap(),
            )
        });

        record.sig = SigCode(*b"ALCH");
        record.fields.retain(|field| {
            matches!(
                field.sig.as_str(),
                "EDID"
                    | "OBND"
                    | "PTRN"
                    | "FULL"
                    | "KSIZ"
                    | "KWDA"
                    | "MODL"
                    | "MODT"
                    | "MODC"
                    | "MODS"
                    | "DESC"
                    | "YNAM"
                    | "ZNAM"
            )
        });
        let mut enit = Vec::with_capacity(5);
        push_field(
            &mut enit,
            interner,
            "value",
            FieldValue::Bytes(SmallVec::from_slice(&value)),
        );
        push_field(
            &mut enit,
            interner,
            "flags",
            FieldValue::Bytes(SmallVec::from_slice(&ALCH_NO_AUTO_CALC.to_le_bytes())),
        );
        push_field(&mut enit, interner, "addiction", null_reference());
        push_field(
            &mut enit,
            interner,
            "addiction_chance",
            FieldValue::Bytes(SmallVec::from_slice(&[0; 4])),
        );
        push_field(&mut enit, interner, "sound_consume", sound_consume);
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Bytes(SmallVec::from_slice(&weight)),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"ENIT"),
            value: FieldValue::Struct(enit),
        });
    }
}
