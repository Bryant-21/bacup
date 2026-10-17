#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AttachResult {
    Changed,
    AlreadyPresent,
    Conflict(&'static str),
}

const VMAD_VERSION: u16 = 6;
const VMAD_OBJECT_FORMAT: u16 = 2;

struct VmadReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> VmadReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.offset.checked_add(count)?;
        let value = self.bytes.get(self.offset..end)?;
        self.offset = end;
        Some(value)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|bytes| bytes[0])
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn string_bytes(&mut self) -> Option<&'a [u8]> {
        let length = self.u16()? as usize;
        self.take(length)
    }

    fn nonnegative_count(&mut self) -> Option<usize> {
        usize::try_from(self.i32()?).ok()
    }
}

fn skip_struct(reader: &mut VmadReader<'_>, object_format: u16) -> Option<()> {
    let count = reader.nonnegative_count()?;
    for _ in 0..count {
        reader.string_bytes()?;
        let property_type = reader.u8()?;
        reader.u8()?;
        skip_property_value(reader, property_type, object_format)?;
    }
    Some(())
}

fn skip_property_value(
    reader: &mut VmadReader<'_>,
    property_type: u8,
    object_format: u16,
) -> Option<()> {
    match property_type {
        0 | 6 => Some(()),
        1 => {
            reader.take(8)?;
            Some(())
        }
        2 => {
            reader.string_bytes()?;
            Some(())
        }
        3 | 4 => {
            reader.take(4)?;
            Some(())
        }
        5 => {
            reader.take(1)?;
            Some(())
        }
        7 => skip_struct(reader, object_format),
        11 => {
            let count = reader.nonnegative_count()?;
            reader.take(count.checked_mul(8)?)?;
            Some(())
        }
        12 => {
            let count = reader.nonnegative_count()?;
            for _ in 0..count {
                reader.string_bytes()?;
            }
            Some(())
        }
        13 | 14 => {
            let count = reader.nonnegative_count()?;
            reader.take(count.checked_mul(4)?)?;
            Some(())
        }
        15 => {
            let count = reader.nonnegative_count()?;
            reader.take(count)?;
            Some(())
        }
        16 => {
            reader.take(4)?;
            Some(())
        }
        17 => {
            let count = reader.nonnegative_count()?;
            for _ in 0..count {
                skip_struct(reader, object_format)?;
            }
            Some(())
        }
        _ => None,
    }
}

pub(crate) fn map_top_level_script_names(
    existing: &mut Vec<u8>,
    map_name: &mut dyn FnMut(&str) -> Option<String>,
) -> Option<usize> {
    let mut reader = VmadReader::new(existing);
    if !matches!(reader.u16()?, 5 | 6) {
        return None;
    }
    let object_format = reader.u16()?;
    if !matches!(object_format, 1 | 2) {
        return None;
    }
    let count = reader.u16()?;
    let mut replacements = Vec::new();
    for _ in 0..count {
        let start = reader.offset;
        let name = std::str::from_utf8(reader.string_bytes()?).ok()?;
        let end = reader.offset;
        if let Some(name) = map_name(name) {
            let length = u16::try_from(name.len()).ok()?;
            let mut encoded = length.to_le_bytes().to_vec();
            encoded.extend_from_slice(name.as_bytes());
            replacements.push((start..end, encoded));
        }
        reader.u8()?;
        let properties = reader.u16()?;
        for _ in 0..properties {
            reader.string_bytes()?;
            let kind = reader.u8()?;
            reader.u8()?;
            skip_property_value(&mut reader, kind, object_format)?;
        }
    }
    let changed = replacements.len();
    for (span, encoded) in replacements.into_iter().rev() {
        existing.splice(span, encoded);
    }
    Some(changed)
}

pub(crate) fn add_top_level_object_properties(
    existing: &mut Vec<u8>,
    properties: &mut dyn FnMut(&str) -> Option<Vec<(String, u32)>>,
) -> Option<()> {
    let mut reader = VmadReader::new(existing);
    if reader.u16()? != VMAD_VERSION || reader.u16()? != VMAD_OBJECT_FORMAT { return None; }
    let count = reader.u16()?;
    let mut replacements = Vec::new();
    for _ in 0..count {
        let name = std::str::from_utf8(reader.string_bytes()?).ok()?;
        let additions = properties(name)?;
        reader.u8()?;
        let start = reader.offset;
        let property_count = reader.u16()?;
        let mut present = std::collections::BTreeSet::new();
        for _ in 0..property_count {
            present.insert(std::str::from_utf8(reader.string_bytes()?).ok()?.to_ascii_lowercase());
            let kind = reader.u8()?;
            reader.u8()?;
            skip_property_value(&mut reader, kind, VMAD_OBJECT_FORMAT)?;
        }
        if additions.is_empty() { continue; }
        let new_count = property_count.checked_add(u16::try_from(additions.len()).ok()?)?;
        let mut encoded = new_count.to_le_bytes().to_vec();
        encoded.extend_from_slice(&existing[start + 2..reader.offset]);
        for (property, raw_form_id) in additions {
            if !present.insert(property.to_ascii_lowercase()) { return None; }
            encoded.extend_from_slice(&u16::try_from(property.len()).ok()?.to_le_bytes());
            encoded.extend_from_slice(property.as_bytes());
            encoded.extend_from_slice(&[1, 1]);
            encoded.extend_from_slice(&0u16.to_le_bytes());
            encoded.extend_from_slice(&(-1i16).to_le_bytes());
            encoded.extend_from_slice(&raw_form_id.to_le_bytes());
        }
        replacements.push((start..reader.offset, encoded));
    }
    for (span, encoded) in replacements.into_iter().rev() { existing.splice(span, encoded); }
    Some(())
}

pub(crate) fn retain_standalone_scripts(
    existing: &mut Vec<u8>,
    keep: &dyn Fn(&str) -> bool,
) -> Option<usize> {
    let mut reader = VmadReader::new(existing);
    if !matches!(reader.u16()?, 5 | 6) { return None; }
    let format = reader.u16()?;
    if !matches!(format, 1 | 2) { return None; }
    let count = reader.u16()?;
    let mut spans = Vec::new();
    for _ in 0..count {
        let start = reader.offset;
        let name = std::str::from_utf8(reader.string_bytes()?).ok()?;
        reader.u8()?;
        for _ in 0..reader.u16()? {
            reader.string_bytes()?;
            let kind = reader.u8()?;
            reader.u8()?;
            skip_property_value(&mut reader, kind, format)?;
        }
        if keep(name) { spans.push(start..reader.offset); }
    }
    let mut output = existing[..4].to_vec();
    output.extend_from_slice(&u16::try_from(spans.len()).ok()?.to_le_bytes());
    for span in &spans { output.extend_from_slice(&existing[span.clone()]); }
    *existing = output;
    Some(spans.len())
}

/// True when any top-level script in the VMAD satisfies the predicate; `None`
/// when the VMAD could not be walked.
pub(crate) fn has_top_level_script(
    bytes: &[u8],
    script_matches: &dyn Fn(&[u8]) -> bool,
) -> Option<bool> {
    let mut reader = VmadReader::new(bytes);
    let version = reader.u16()?;
    let object_format = reader.u16()?;
    if version != VMAD_VERSION || object_format != VMAD_OBJECT_FORMAT {
        return None;
    }
    let script_count = reader.u16()?;
    let mut found = false;
    for _ in 0..script_count {
        let script_name = reader.string_bytes()?;
        reader.u8()?;
        let property_count = reader.u16()?;
        for _ in 0..property_count {
            reader.string_bytes()?;
            let property_type = reader.u8()?;
            reader.u8()?;
            skip_property_value(&mut reader, property_type, object_format)?;
        }
        found |= script_matches(script_name);
    }
    Some(found)
}

/// Byte ranges of the length-prefixed name of every `property_name` property
/// carried by a top-level script the predicate accepts.
fn top_level_property_name_spans(
    bytes: &[u8],
    script_matches: &dyn Fn(&[u8]) -> bool,
    property_name: &[u8],
) -> Option<Vec<std::ops::Range<usize>>> {
    let mut reader = VmadReader::new(bytes);
    let version = reader.u16()?;
    let object_format = reader.u16()?;
    if version != VMAD_VERSION || object_format != VMAD_OBJECT_FORMAT {
        return None;
    }
    let script_count = reader.u16()?;
    let mut spans = Vec::new();
    for _ in 0..script_count {
        let script_name = reader.string_bytes()?;
        reader.u8()?;
        let property_count = reader.u16()?;
        let matched = script_matches(script_name);
        for _ in 0..property_count {
            let start = reader.offset;
            let name = reader.string_bytes()?;
            let end = reader.offset;
            let property_type = reader.u8()?;
            reader.u8()?;
            skip_property_value(&mut reader, property_type, object_format)?;
            if matched && name.eq_ignore_ascii_case(property_name) {
                spans.push(start..end);
            }
        }
    }
    Some(spans)
}

/// Rename a property on every top-level script the predicate accepts, returning
/// how many were renamed, or `None` when the VMAD could not be walked.
///
/// Property names are length-prefixed, so a rename that changes length splices
/// rather than overwrites; spans are applied back to front so the earlier
/// offsets stay valid.
pub(crate) fn rename_top_level_script_property(
    existing: &mut Vec<u8>,
    script_matches: &dyn Fn(&[u8]) -> bool,
    old_name: &str,
    new_name: &str,
) -> Option<usize> {
    let spans = top_level_property_name_spans(existing, script_matches, old_name.as_bytes())?;
    let length = u16::try_from(new_name.len()).ok()?;
    let mut encoded = Vec::with_capacity(2 + new_name.len());
    encoded.extend_from_slice(&length.to_le_bytes());
    encoded.extend_from_slice(new_name.as_bytes());
    let renamed = spans.len();
    for span in spans.into_iter().rev() {
        existing.splice(span, encoded.iter().copied());
    }
    Some(renamed)
}

fn top_level_script_layout<'a>(
    bytes: &'a [u8],
    script_name: &str,
) -> Option<(u16, usize, Vec<&'a [u8]>)> {
    let mut reader = VmadReader::new(bytes);
    let version = reader.u16()?;
    let object_format = reader.u16()?;
    if version != VMAD_VERSION || object_format != VMAD_OBJECT_FORMAT {
        return None;
    }
    let script_count = reader.u16()?;
    let mut matching = Vec::new();
    for _ in 0..script_count {
        let start = reader.offset;
        let name = reader.string_bytes()?;
        reader.u8()?;
        let property_count = reader.u16()?;
        for _ in 0..property_count {
            reader.string_bytes()?;
            let property_type = reader.u8()?;
            reader.u8()?;
            skip_property_value(&mut reader, property_type, object_format)?;
        }
        if name == script_name.as_bytes() {
            matching.push(&bytes[start..reader.offset]);
        }
    }
    Some((script_count, reader.offset, matching))
}

pub(crate) fn attach_script_bytes(
    existing: &mut Vec<u8>,
    script_name: &str,
    script_vmad: &[u8],
) -> AttachResult {
    if script_vmad.len() < 6 {
        return AttachResult::Conflict("generated_vmad_invalid");
    }
    let Some((script_count, insert_at, matching)) = top_level_script_layout(existing, script_name)
    else {
        return AttachResult::Conflict("unsupported_or_malformed_vmad");
    };
    match matching.as_slice() {
        [] => {}
        [existing_script] if *existing_script == &script_vmad[6..] => {
            return AttachResult::AlreadyPresent;
        }
        [_] => return AttachResult::Conflict("same_script_different_binding"),
        _ => return AttachResult::Conflict("duplicate_script_binding"),
    }
    let Some(next_count) = script_count.checked_add(1) else {
        return AttachResult::Conflict("script_count_overflow");
    };
    existing[4..6].copy_from_slice(&next_count.to_le_bytes());
    existing.splice(insert_at..insert_at, script_vmad[6..].iter().copied());
    AttachResult::Changed
}
