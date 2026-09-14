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
