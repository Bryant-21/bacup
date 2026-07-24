fn make_ctx(interner: &StringInterner) -> PairCtx<'_> {
    PairCtx { interner }
}

fn make_record(sig: &str, interner: &StringInterner) -> Record {
    let fk = FormKey::parse("000800@FalloutNV.esm", interner).unwrap();
    Record::new(SigCode::from_str(sig).unwrap(), fk)
}

fn push_field(record: &mut Record, sig: &str, value: FieldValue) {
    record.fields.push(FieldEntry {
        sig: crate::ids::SubrecordSig::from_str(sig).unwrap(),
        value,
    });
}

fn legacy_magic_mapper(interner: &StringInterner) -> FormKeyMapper<'_> {
    FormKeyMapper::new(
        std::iter::empty(),
        MapperOptions {
            output_plugin_name: "Out.esm".into(),
            source_plugin_name: "FalloutNV.esm".into(),
            ..MapperOptions::default()
        },
        interner,
    )
}

fn bytes_from_hex(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len() % 2, 0);
    (0..hex.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap())
        .collect()
}

fn raw_field<'a>(record: &'a Record, sig: &str) -> &'a [u8] {
    let fields: Vec<_> = record
        .fields
        .iter()
        .filter(|entry| entry.sig.as_str() == sig)
        .collect();
    assert_eq!(fields.len(), 1, "expected exactly one {sig}");
    let FieldValue::Bytes(bytes) = &fields[0].value else {
        panic!("{sig} must be raw bytes");
    };
    bytes.as_slice()
}

fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap())
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn f32_at(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

// -------------------------------------------------------------------------
// Behavior 1: global field drop (SCRI)
// -------------------------------------------------------------------------
