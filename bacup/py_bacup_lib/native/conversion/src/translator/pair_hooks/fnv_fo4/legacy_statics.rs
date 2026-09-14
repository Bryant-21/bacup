use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};

const URBAN_STATUE_MODEL: &str = "Architecture\\Urban\\Statues\\UrbanStatue01.NIF";
const HOUSE_CHIMNEY_MODEL: &str = "Architecture\\Suburban\\HouseChimney01.NIF";

pub(super) fn repair_legacy_static_model(
    interner: &crate::sym::StringInterner,
    record: &mut Record,
) {
    if record.sig.0 != *b"STAT" {
        return;
    }
    let Some(editor_id) = record.eid.and_then(|eid| interner.resolve(eid)) else {
        return;
    };
    let replacement = if editor_id.eq_ignore_ascii_case("UrbanStatue01") {
        URBAN_STATUE_MODEL
    } else if editor_id.eq_ignore_ascii_case("HouseChimney01") {
        HOUSE_CHIMNEY_MODEL
    } else {
        return;
    };

    if let Some(model) = record
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"MODL")
    {
        model.value = FieldValue::String(interner.intern(replacement));
        return;
    }

    record.fields.push(FieldEntry {
        sig: SubrecordSig::from_str("MODL").expect("valid MODL signature"),
        value: FieldValue::String(interner.intern(replacement)),
    });
}
