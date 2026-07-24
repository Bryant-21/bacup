use crate::record::FieldValue;

pub(super) fn struct_value<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &crate::sym::StringInterner,
) -> Option<&'a FieldValue> {
    fields
        .iter()
        .find(|(key, _)| interner.resolve(*key) == Some(name))
        .map(|(_, value)| value)
}
