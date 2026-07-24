//! Preserve radio-track Scene object IDs in an integer property that Papyrus can
//! resolve after load. FO4 can register the converted SCEN records, but the
//! `songsData[].Track` VM objects from FO76 arrive as unbound objects at runtime.

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::session::PluginSession;

const RADIO_SCRIPT: &[u8] = b"RadioGeneral_MasterScript";
const SONGS_DATA_PROPERTY: &[u8] = b"songsData";
const SONG_FORM_IDS_PROPERTY: &[u8] = b"songFormIDs";
const TRACK_MEMBER: &[u8] = b"Track";

pub struct RepairRadioScenePropertiesFixup;

impl Fixup for RepairRadioScenePropertiesFixup {
    fn name(&self) -> &'static str {
        "repair_radio_scene_properties"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        true
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let own_master_index = session.target_masters().len() as u32;
        if own_master_index > u8::MAX as u32 {
            return Ok(report);
        }

        let qust_sig =
            SigCode::from_str("QUST").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let present = session
            .target_signatures()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if !present.iter().any(|sig| sig.as_str() == "QUST") {
            return Ok(report);
        }

        let vmad_only = ["VMAD"];
        let form_keys = session
            .form_keys_of_sig(qust_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        for form_key in form_keys {
            if !session
                .record_has_any_subrecord(&form_key, &vmad_only)
                .unwrap_or(false)
            {
                continue;
            }
            let changed = session
                .patch_all_subrecords_bytes(&form_key, "VMAD", |bytes| {
                    repair_radio_vmad(bytes, own_master_index)
                })
                .map_err(|e| FixupError::HandleError(e.to_string()))?;
            if changed > 0 {
                report.records_changed += 1;
            }
        }

        Ok(report)
    }
}

#[derive(Debug)]
struct PropertyInsertion {
    property_count_offset: usize,
    insert_offset: usize,
    form_ids: Vec<i32>,
}

fn repair_radio_vmad(data: &mut Vec<u8>, own_master_index: u32) -> bool {
    let Some(mut insertions) = find_property_insertions(data, own_master_index) else {
        return false;
    };
    if insertions.is_empty() {
        return false;
    }

    insertions.sort_unstable_by_key(|insertion| insertion.insert_offset);
    for insertion in insertions.into_iter().rev() {
        let Some(property_count) = read_u16(data, insertion.property_count_offset) else {
            return false;
        };
        let Some(property_count) = property_count.checked_add(1) else {
            return false;
        };
        data[insertion.property_count_offset..insertion.property_count_offset + 2]
            .copy_from_slice(&property_count.to_le_bytes());

        let mut property = Vec::new();
        push_string(&mut property, SONG_FORM_IDS_PROPERTY);
        property.push(13); // Int[]
        property.push(1); // edited
        property.extend_from_slice(&(insertion.form_ids.len() as i32).to_le_bytes());
        for form_id in insertion.form_ids {
            property.extend_from_slice(&form_id.to_le_bytes());
        }
        data.splice(insertion.insert_offset..insertion.insert_offset, property);
    }
    true
}

fn find_property_insertions(data: &[u8], own_master_index: u32) -> Option<Vec<PropertyInsertion>> {
    let mut reader = Reader::new(data);
    let version = reader.read_u16()?;
    let object_format = reader.read_u16()?;
    let script_count = reader.read_u16()?;
    if version == 0 || !matches!(object_format, 1 | 2) {
        return None;
    }

    let mut insertions = Vec::new();
    for _ in 0..script_count {
        let script_name = reader.read_string()?;
        reader.advance(1)?; // flags
        let property_count_offset = reader.offset;
        let property_count = reader.read_u16()?;
        if property_count == u16::MAX {
            return None;
        }
        let is_radio_script = eq_ascii_case(script_name, RADIO_SCRIPT);
        let mut has_form_ids = false;
        let mut form_ids = None;

        for _ in 0..property_count {
            let property_name = reader.read_string()?;
            let property_type = reader.read_u8()?;
            reader.advance(1)?; // flags

            if is_radio_script
                && eq_ascii_case(property_name, SONGS_DATA_PROPERTY)
                && property_type == 17
            {
                form_ids = Some(read_song_form_ids(
                    &mut reader,
                    object_format,
                    own_master_index,
                )?);
            } else {
                if is_radio_script && eq_ascii_case(property_name, SONG_FORM_IDS_PROPERTY) {
                    has_form_ids = true;
                }
                skip_property_value(&mut reader, property_type, object_format)?;
            }
        }

        if is_radio_script && !has_form_ids {
            if let Some(form_ids) = form_ids {
                insertions.push(PropertyInsertion {
                    property_count_offset,
                    insert_offset: reader.offset,
                    form_ids,
                });
            }
        }
    }
    Some(insertions)
}

fn read_song_form_ids(
    reader: &mut Reader<'_>,
    object_format: u16,
    own_master_index: u32,
) -> Option<Vec<i32>> {
    let count = reader.read_count()?;
    let mut form_ids = Vec::with_capacity(count);
    for _ in 0..count {
        let member_count = reader.read_count()?;
        let mut form_id = 0i32;
        for _ in 0..member_count {
            let member_name = reader.read_string()?;
            let member_type = reader.read_u8()?;
            reader.advance(1)?; // flags
            if eq_ascii_case(member_name, TRACK_MEMBER) && member_type == 1 {
                let raw = read_object(reader, object_format)?;
                if raw >> 24 == own_master_index {
                    form_id = (raw & 0x00ff_ffff) as i32;
                }
            } else {
                skip_property_value(reader, member_type, object_format)?;
            }
        }
        form_ids.push(form_id);
    }
    Some(form_ids)
}

fn skip_property_value(
    reader: &mut Reader<'_>,
    property_type: u8,
    object_format: u16,
) -> Option<()> {
    match property_type {
        0 | 6 => Some(()),
        1 => {
            read_object(reader, object_format)?;
            Some(())
        }
        2 => {
            reader.read_string()?;
            Some(())
        }
        3 | 4 => reader.advance(4),
        5 => reader.advance(1),
        7 => skip_struct(reader, object_format),
        11 => {
            let count = reader.read_count()?;
            for _ in 0..count {
                read_object(reader, object_format)?;
            }
            Some(())
        }
        12 => {
            let count = reader.read_count()?;
            for _ in 0..count {
                reader.read_string()?;
            }
            Some(())
        }
        13 | 14 => {
            let count = reader.read_count()?;
            reader.advance(count.checked_mul(4)?)
        }
        15 => {
            let count = reader.read_count()?;
            reader.advance(count)
        }
        16 => reader.advance(4),
        17 => {
            let count = reader.read_count()?;
            for _ in 0..count {
                skip_struct(reader, object_format)?;
            }
            Some(())
        }
        _ => None,
    }
}

fn skip_struct(reader: &mut Reader<'_>, object_format: u16) -> Option<()> {
    let member_count = reader.read_count()?;
    for _ in 0..member_count {
        reader.read_string()?;
        let member_type = reader.read_u8()?;
        reader.advance(1)?;
        skip_property_value(reader, member_type, object_format)?;
    }
    Some(())
}

fn read_object(reader: &mut Reader<'_>, object_format: u16) -> Option<u32> {
    if object_format == 2 {
        reader.advance(4)?;
        reader.read_u32()
    } else if object_format == 1 {
        let form_id = reader.read_u32()?;
        reader.advance(4)?;
        Some(form_id)
    } else {
        None
    }
}

fn eq_ascii_case(left: &[u8], right: &[u8]) -> bool {
    left.eq_ignore_ascii_case(right)
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    let bytes = data.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn push_string(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u16).to_le_bytes());
    out.extend_from_slice(value);
}

struct Reader<'a> {
    data: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, offset: 0 }
    }

    fn advance(&mut self, count: usize) -> Option<()> {
        let next = self.offset.checked_add(count)?;
        if next > self.data.len() {
            return None;
        }
        self.offset = next;
        Some(())
    }

    fn read_u8(&mut self) -> Option<u8> {
        let value = self.data.get(self.offset).copied()?;
        self.offset = self.offset.checked_add(1)?;
        Some(value)
    }

    fn read_u16(&mut self) -> Option<u16> {
        let bytes = self.data.get(self.offset..self.offset.checked_add(2)?)?;
        self.offset = self.offset.checked_add(2)?;
        Some(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    fn read_u32(&mut self) -> Option<u32> {
        let bytes = self.data.get(self.offset..self.offset.checked_add(4)?)?;
        self.offset = self.offset.checked_add(4)?;
        Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn read_count(&mut self) -> Option<usize> {
        usize::try_from(self.read_u32()? as i32).ok()
    }

    fn read_string(&mut self) -> Option<&'a [u8]> {
        let length = self.read_u16()? as usize;
        let end = self.offset.checked_add(length)?;
        let value = self.data.get(self.offset..end)?;
        self.offset = end;
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_object(out: &mut Vec<u8>, raw: u32) {
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0i16.to_le_bytes());
        out.extend_from_slice(&raw.to_le_bytes());
    }

    fn radio_vmad(script_name: &[u8], tracks: &[u32]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&5u16.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        push_string(&mut out, script_name);
        out.push(0);
        out.extend_from_slice(&1u16.to_le_bytes());
        push_string(&mut out, SONGS_DATA_PROPERTY);
        out.push(17);
        out.push(1);
        out.extend_from_slice(&(tracks.len() as i32).to_le_bytes());
        for (index, track) in tracks.iter().enumerate() {
            out.extend_from_slice(&2i32.to_le_bytes());
            push_string(&mut out, b"title");
            out.push(2);
            out.push(1);
            push_string(&mut out, format!("Track {index}").as_bytes());
            push_string(&mut out, TRACK_MEMBER);
            out.push(1);
            out.push(1);
            push_object(&mut out, *track);
        }
        out
    }

    fn int_array_property(data: &[u8], wanted: &[u8]) -> Option<Vec<i32>> {
        let mut reader = Reader::new(data);
        reader.read_u16()?;
        let object_format = reader.read_u16()?;
        let script_count = reader.read_u16()?;
        for _ in 0..script_count {
            reader.read_string()?;
            reader.advance(1)?;
            let property_count = reader.read_u16()?;
            for _ in 0..property_count {
                let name = reader.read_string()?;
                let property_type = reader.read_u8()?;
                reader.advance(1)?;
                if eq_ascii_case(name, wanted) && property_type == 13 {
                    let count = reader.read_count()?;
                    let mut values = Vec::with_capacity(count);
                    for _ in 0..count {
                        values.push(reader.read_u32()? as i32);
                    }
                    return Some(values);
                }
                skip_property_value(&mut reader, property_type, object_format)?;
            }
        }
        None
    }

    #[test]
    fn adds_local_scene_ids_parallel_to_song_structs() {
        let mut data = radio_vmad(RADIO_SCRIPT, &[0x084f_c07a, 0x0839_9c31, 0x0003_04f0]);

        assert!(repair_radio_vmad(&mut data, 8));
        assert_eq!(
            int_array_property(&data, SONG_FORM_IDS_PROPERTY),
            Some(vec![0x4f_c07a, 0x39_9c31, 0])
        );
        assert_eq!(read_u16(&data, 6 + 2 + RADIO_SCRIPT.len() + 1), Some(2));
    }

    #[test]
    fn second_pass_is_byte_identical() {
        let mut data = radio_vmad(RADIO_SCRIPT, &[0x084f_c07a]);
        assert!(repair_radio_vmad(&mut data, 8));
        let repaired = data.clone();

        assert!(!repair_radio_vmad(&mut data, 8));
        assert_eq!(data, repaired);
    }

    #[test]
    fn ignores_other_scripts_and_malformed_vmad() {
        let mut other = radio_vmad(b"OtherScript", &[0x084f_c07a]);
        let original = other.clone();
        assert!(!repair_radio_vmad(&mut other, 8));
        assert_eq!(other, original);

        let mut malformed = radio_vmad(RADIO_SCRIPT, &[0x084f_c07a]);
        malformed.truncate(malformed.len() - 3);
        let original = malformed.clone();
        assert!(!repair_radio_vmad(&mut malformed, 8));
        assert_eq!(malformed, original);
    }
}
