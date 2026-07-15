//! FO76 `MISC.MCQP` -> FO4 `MISC.CVPA` component rows.
//!
//! FO76 stores each row as `(component, ComponentQuantity* keyword)`. FO4 stores
//! the same component list as `(component, numeric_count)`.

use super::{Transform, TransformCtx, TransformError};
use crate::ids::FormKey;
use crate::record::FieldValue;
use crate::translator::maps::YamlValue;

pub struct Fo76MiscComponentsTransform;

impl Transform for Fo76MiscComponentsTransform {
    fn name(&self) -> &'static str {
        "fo76_misc_components"
    }

    fn apply(
        &self,
        ctx: &mut TransformCtx<'_>,
        value: &mut FieldValue,
        config: &YamlValue,
    ) -> Result<(), TransformError> {
        let FieldValue::Bytes(bytes) = value else {
            return Ok(());
        };
        if bytes.len() % 8 != 0 {
            return Ok(());
        }

        let source_esm = config
            .get("source_esm")
            .and_then(|v| v.as_str())
            .unwrap_or("SeventySix.esm");
        let source_plugin = ctx.interner.intern(source_esm);
        let component_key = ctx.interner.intern("ComponentsComponent");
        let count_key = ctx.interner.intern("ComponentsCount");

        let mut rows = Vec::with_capacity(bytes.len() / 8);
        for row in bytes.chunks_exact(8) {
            let component_raw = u32::from_le_bytes([row[0], row[1], row[2], row[3]]);
            let quantity_raw = u32::from_le_bytes([row[4], row[5], row[6], row[7]]);
            let component = component_value(component_raw, source_plugin);
            let count = quantity_keyword_count(quantity_raw).unwrap_or(1);

            rows.push(FieldValue::Struct(vec![
                (component_key, component),
                (count_key, FieldValue::Uint(u64::from(count))),
            ]));
        }

        *value = FieldValue::List(rows);
        Ok(())
    }
}

fn component_value(raw: u32, source_plugin: crate::sym::Sym) -> FieldValue {
    let local = raw & 0x00ff_ffff;
    if local == 0 {
        FieldValue::Uint(0)
    } else {
        FieldValue::FormKey(FormKey {
            local,
            plugin: source_plugin,
        })
    }
}

fn quantity_keyword_count(raw: u32) -> Option<u32> {
    match raw & 0x00ff_ffff {
        0x0015ff => Some(1),  // ComponentQuantity_Scrap_Singular
        0x11c726 => Some(1),  // ComponentQuantityLow
        0x11c725 => Some(2),  // ComponentQuantityMedium
        0x11c729 => Some(3),  // ComponentQuantityHigh
        0x06a3ae => Some(1),  // ComponentQuantityRare
        0x3d0f71 => Some(10), // ComponentQuantityBulk
        0x59f11e => Some(10), // ComponentQuantity_CustomerService_Bulk
        0x5c3007 => Some(1),  // ATX_ComponentQuantity_Scrapball_Level1_SP1
        0x5c3005 => Some(2),  // ATX_ComponentQuantity_Scrapball_Level2
        0x5c3006 => Some(3),  // ATX_ComponentQuantity_Scrapball_Level3
        0x8b0962 => Some(1),  // PTS_ComponentQuantity
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym::StringInterner;

    fn ctx(interner: &StringInterner) -> TransformCtx<'_> {
        TransformCtx { interner }
    }

    #[test]
    fn converts_low_and_medium_rows_to_fo4_component_counts() {
        let interner = StringInterner::new();
        let mut value = FieldValue::Bytes(
            [
                0x0001_fa8c_u32.to_le_bytes(),
                0x0011_c726_u32.to_le_bytes(),
                0x0001_fa96_u32.to_le_bytes(),
                0x0011_c725_u32.to_le_bytes(),
            ]
            .concat()
            .into(),
        );

        Fo76MiscComponentsTransform
            .apply(&mut ctx(&interner), &mut value, &serde_json::json!({}))
            .unwrap();

        let FieldValue::List(rows) = value else {
            panic!("MCQP should become a CVPA row list");
        };
        assert_eq!(rows.len(), 2);
        assert_component_row(&interner, &rows[0], 0x0001_fa8c, 1);
        assert_component_row(&interner, &rows[1], 0x0001_fa96, 2);
    }

    #[test]
    fn leaves_malformed_payload_raw() {
        let interner = StringInterner::new();
        let original = FieldValue::Bytes([1, 2, 3].as_slice().into());
        let mut value = original.clone();

        Fo76MiscComponentsTransform
            .apply(&mut ctx(&interner), &mut value, &serde_json::json!({}))
            .unwrap();

        assert_eq!(value, original);
    }

    #[test]
    fn maps_all_known_fo76_quantity_keywords() {
        for (keyword, count) in [
            (0x0015ff, 1),
            (0x11c726, 1),
            (0x11c725, 2),
            (0x11c729, 3),
            (0x06a3ae, 1),
            (0x3d0f71, 10),
            (0x59f11e, 10),
            (0x5c3007, 1),
            (0x5c3005, 2),
            (0x5c3006, 3),
            (0x8b0962, 1),
        ] {
            assert_eq!(quantity_keyword_count(keyword), Some(count));
        }
    }

    fn assert_component_row(
        interner: &StringInterner,
        row: &FieldValue,
        expected_local: u32,
        expected_count: u32,
    ) {
        let FieldValue::Struct(fields) = row else {
            panic!("component row should be a struct");
        };
        let component = fields
            .iter()
            .find(|(key, _)| interner.resolve(*key) == Some("ComponentsComponent"))
            .map(|(_, value)| value)
            .expect("component field");
        let count = fields
            .iter()
            .find(|(key, _)| interner.resolve(*key) == Some("ComponentsCount"))
            .map(|(_, value)| value)
            .expect("count field");

        let FieldValue::FormKey(component_fk) = component else {
            panic!("component should be a form key");
        };
        assert_eq!(component_fk.local, expected_local);
        assert_eq!(count, &FieldValue::Uint(u64::from(expected_count)));
    }
}
