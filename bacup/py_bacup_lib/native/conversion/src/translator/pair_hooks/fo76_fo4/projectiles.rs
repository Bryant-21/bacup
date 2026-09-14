use super::*;

impl Fo76Fo4Hook {
    pub(super) fn normalize_weapon_cone_force(ctx: &PairCtx<'_>, record: &mut Record) {
        if record.sig.0 != *b"PROJ" {
            return;
        }
        let is_query_layer = |key: FormKey| {
            key.local == 0x5B74D0
                && ctx.interner.resolve(key.plugin).is_some_and(|name| {
                    name.eq_ignore_ascii_case(FO76_MASTER_NAME)
                })
        };
        for entry in &mut record.fields {
            if entry.sig.0 != *b"DNAM" {
                continue;
            }
            match &mut entry.value {
                FieldValue::Bytes(bytes) if bytes.len() >= 88 => {
                    let kind = u16::from_le_bytes([bytes[2], bytes[3]]);
                    let layer = u32::from_le_bytes(bytes[84..88].try_into().unwrap());
                    if kind == 16
                        && ctx.resolve_source_form_id(layer).is_some_and(is_query_layer)
                    {
                        // Actor damage/stagger is supplied by the paired F4SE cone adapter.
                        // Physical impulse on this query shape is a separate, unwanted effect.
                        bytes[48..52].copy_from_slice(&0.0_f32.to_le_bytes());
                    }
                }
                FieldValue::Struct(fields) => {
                    let field = |name| fields.iter().find(|(key, _)| {
                        Self::struct_field_name_is(ctx.interner, *key, name)
                    }).map(|(_, value)| value);
                    let cone = match field("Type") {
                        Some(FieldValue::Uint(16) | FieldValue::Int(16)) => true,
                        Some(FieldValue::String(value)) => ctx.interner.resolve(*value)
                            .is_some_and(|name| name.eq_ignore_ascii_case("Cone")),
                        _ => false,
                    };
                    let layer = matches!(field("CollisionLayer"),
                        Some(FieldValue::FormKey(key)) if is_query_layer(*key));
                    if cone && layer {
                        if let Some((_, force)) = fields.iter_mut().find(|(key, _)| {
                            Self::struct_field_name_is(ctx.interner, *key, "ImpactForce")
                        }) {
                            *force = FieldValue::Float(0.0);
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
