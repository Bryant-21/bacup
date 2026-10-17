use serde::Serialize;

use crate::formkey_mapper::FormKeyMapper;
use rustc_hash::{FxHashMap, FxHashSet};

use super::super::STARFIELD_METERS_TO_FO4_UNITS;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OmodInclude {
    pub raw_mod: u32,
    pub minimum_level: u8,
    pub optional: bool,
    pub dont_use_all: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct OmodProperty {
    pub value_type: u8,
    pub function: u8,
    pub property: [u8; 4],
    pub value1: u32,
    pub value2: u32,
    pub step: f32,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SourceWeaponOmod {
    pub unknown_header: [u8; 2],
    pub instance_class: String,
    pub max_rank: u8,
    pub level_tier_offset: u8,
    pub attach_point_raw: u32,
    pub attach_parent_slots_raw: Vec<u32>,
    pub unknown_item: u32,
    pub includes: Vec<OmodInclude>,
    pub properties: Vec<OmodProperty>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct TemplateInclude {
    pub raw_mod: u32,
    pub attach_point_index: u8,
    pub optional: bool,
    pub dont_use_all: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SourceObjectTemplate {
    pub level_min: u16,
    pub level_max: u16,
    pub parent_combination_index: i16,
    pub default: bool,
    pub keywords_raw: Vec<u32>,
    pub min_level_for_ranks: u8,
    pub alt_levels_per_tier: u8,
    pub includes: Vec<TemplateInclude>,
    pub properties: Vec<OmodProperty>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or("Starfield OMOD size overflow")?;
        let result = self
            .bytes
            .get(self.offset..end)
            .ok_or("Starfield OMOD DATA is truncated")?;
        self.offset = end;
        Ok(result)
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn boolean(&mut self) -> Result<bool, String> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(format!("Starfield OMOD invalid bool {value}")),
        }
    }
    fn properties(&mut self, count: usize) -> Result<Vec<OmodProperty>, String> {
        let mut properties = Vec::with_capacity(count);
        for _ in 0..count {
            let value_type = self.u8()?;
            if self.take(3)? != [0; 3] {
                return Err("Starfield OMOD nonzero property padding".into());
            }
            let function = self.u8()?;
            if self.take(3)? != [0; 3] {
                return Err("Starfield OMOD nonzero function padding".into());
            }
            if value_type > 6 || function > 2 || (value_type == 5 && function != 0) {
                return Err("Starfield OMOD invalid property type/function".into());
            }
            let property = self.take(4)?.try_into().unwrap();
            let value1 = self.u32()?;
            let value2 = self.u32()?;
            if (value_type == 1 && !f32::from_bits(value1).is_finite())
                || (matches!(value_type, 1 | 6) && !f32::from_bits(value2).is_finite())
            {
                return Err("Starfield OMOD nonfinite property value".into());
            }
            let step = f32::from_bits(self.u32()?);
            if !step.is_finite() {
                return Err("Starfield OMOD nonfinite step".into());
            }
            properties.push(OmodProperty {
                value_type,
                function,
                property,
                value1,
                value2,
                step,
            });
        }
        Ok(properties)
    }
}

pub(crate) fn decode_source_object_template(bytes: &[u8]) -> Result<SourceObjectTemplate, String> {
    let mut reader = Reader { bytes, offset: 0 };
    let include_count = reader.u32()? as usize;
    let property_count = reader.u32()? as usize;
    let level_min = u16::from_le_bytes(reader.take(2)?.try_into().unwrap());
    let level_max = u16::from_le_bytes(reader.take(2)?.try_into().unwrap());
    let parent_combination_index = i16::from_le_bytes(reader.take(2)?.try_into().unwrap());
    let default = reader.boolean()?;
    let keyword_count = reader.u32()? as usize;
    let keywords_raw = reader
        .take(
            keyword_count
                .checked_mul(4)
                .ok_or("Starfield template keyword overflow")?,
        )?
        .chunks_exact(4)
        .map(|value| u32::from_le_bytes(value.try_into().unwrap()))
        .collect();
    let min_level_for_ranks = reader.u8()?;
    let alt_levels_per_tier = reader.u8()?;
    let expected = include_count
        .checked_mul(7)
        .and_then(|value| {
            property_count
                .checked_mul(24)
                .and_then(|properties| value.checked_add(properties))
        })
        .ok_or("Starfield template row overflow")?;
    if reader.bytes.len() - reader.offset != expected {
        return Err("Starfield template row counts do not match payload".into());
    }
    let mut includes = Vec::with_capacity(include_count);
    for _ in 0..include_count {
        includes.push(TemplateInclude {
            raw_mod: reader.u32()?,
            attach_point_index: reader.u8()?,
            optional: reader.boolean()?,
            dont_use_all: reader.boolean()?,
        });
    }
    let properties = reader.properties(property_count)?;
    Ok(SourceObjectTemplate {
        level_min,
        level_max,
        parent_combination_index,
        default,
        keywords_raw,
        min_level_for_ranks,
        alt_levels_per_tier,
        includes,
        properties,
    })
}

pub(crate) fn decode_source_omod(bytes: &[u8]) -> Result<SourceWeaponOmod, String> {
    let mut reader = Reader { bytes, offset: 0 };
    let include_count = reader.u32()? as usize;
    let property_count = reader.u32()? as usize;
    let unknown_header = reader.take(2)?.try_into().unwrap();
    let name_length = reader.u32()? as usize;
    let name = reader.take(name_length)?;
    let Some(class) = name.strip_suffix(&[0]) else {
        return Err("Starfield OMOD instance class is not terminated".into());
    };
    let instance_class = std::str::from_utf8(class)
        .map_err(|_| "Starfield OMOD invalid instance class")?
        .to_owned();
    if instance_class != "TESObjectWEAP_InstanceData" {
        return Err(format!(
            "Starfield OMOD is not a weapon instance: {instance_class}"
        ));
    }
    let max_rank = reader.u8()?;
    let level_tier_offset = reader.u8()?;
    let attach_point_raw = reader.u32()?;
    let slot_count = reader.u32()? as usize;
    let slot_bytes = reader.take(
        slot_count
            .checked_mul(4)
            .ok_or("Starfield OMOD slot overflow")?,
    )?;
    let attach_parent_slots_raw = slot_bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
        .collect();
    let unknown_item = reader.u32()?;
    let expected = include_count
        .checked_mul(7)
        .and_then(|value| {
            property_count
                .checked_mul(24)
                .and_then(|properties| value.checked_add(properties))
        })
        .ok_or("Starfield OMOD row overflow")?;
    if reader.bytes.len() - reader.offset != expected {
        return Err("Starfield OMOD DATA row counts do not match payload".into());
    }
    let mut includes = Vec::with_capacity(include_count);
    for _ in 0..include_count {
        includes.push(OmodInclude {
            raw_mod: reader.u32()?,
            minimum_level: reader.u8()?,
            optional: reader.boolean()?,
            dont_use_all: reader.boolean()?,
        });
    }
    let properties = reader.properties(property_count)?;
    Ok(SourceWeaponOmod {
        unknown_header,
        instance_class,
        max_rank,
        level_tier_offset,
        attach_point_raw,
        attach_parent_slots_raw,
        unknown_item,
        includes,
        properties,
    })
}

pub(super) fn fo4_property_id(property: &[u8; 4]) -> Option<u16> {
    Some(match property {
        b"WSPD" => 0,
        b"WREA" => 1,
        b"WMNR" => 2,
        b"WMXR" => 3,
        b"WADL" => 4,
        b"WOOR" => 6,
        b"WCCB" => 8,
        b"WHBV" => 9,
        b"WRNK" => 10,
        b"ACPT" => 12,
        b"GTYP" => 15,
        b"WPLY" => 16,
        b"AUSE" => 17,
        b"WCHR" => 18,
        b"WMCR" => 19,
        b"WEDT" => 21,
        b"WNHO" => 23,
        b"FREP" | b"FTYP" => 25,
        b"WNDP" => 26,
        b"WNPL" => 27,
        b"WDMG" => 28,
        b"GVAL" => 29,
        b"GWEI" => 30,
        b"WKEY" => 31,
        b"WAIM" => 32,
        b"WCO1" => 33,
        b"WCO2" => 34,
        b"WCO3" => 35,
        b"WCO4" => 36,
        b"WRC1" => 39,
        b"WRC2" => 40,
        b"WRC3" => 41,
        b"WRC5" => 43,
        b"WRC8" => 46,
        b"WCO6" => 47,
        b"WSCP" => 48,
        b"ZFOV" => 49,
        b"WFSC" => 50,
        b"WNPR" => 51,
        b"WSLV" => 59,
        b"WIMP" => 60,
        b"AMMO" => 61,
        b"WCRT" => 62,
        b"WBSH" => 63,
        b"WBMT" => 64,
        b"WENC" => 65,
        b"WABS" => 66,
        b"ZOVL" => 68,
        b"ZIMG" => 69,
        b"ZOFX" => 70,
        b"ZOFY" => 71,
        b"ZOFZ" => 72,
        b"WEQS" => 73,
        b"WSLM" => 74,
        b"ALIS" => 75,
        b"WRSP" => 76,
        b"WDTV" => 77,
        b"WACB" => 78,
        b"WOPR" => 80,
        b"WBOL" => 81,
        b"WSTG" => 82,
        b"WSTS" => 83,
        b"WFPS" => 84,
        b"WHIP" => 85,
        b"WMPS" => 87,
        b"WCOL" => 88,
        b"WCDM" => 90,
        b"WDSH" => 92,
        b"WCAT" => 93,
        b"WACV" => 94,
        _ => return None,
    })
}

#[derive(Debug, Serialize)]
pub(crate) struct OmodPropertyPlan {
    pub source: OmodProperty,
    pub target_property: Option<u16>,
    pub unresolved_reason: Option<String>,
}

pub(crate) fn property_plan(source: &SourceWeaponOmod) -> Vec<OmodPropertyPlan> {
    source
        .properties
        .iter()
        .map(|property| {
            let target_property = fo4_property_id(&property.property);
            let unresolved_reason = if target_property.is_none() {
                Some(format!(
                    "source_property_{}_requires_runtime_or_stat_lowering",
                    String::from_utf8_lossy(&property.property)
                ))
            } else if property.value_type == 3 {
                Some("source_string_property_requires_string_table_lowering".into())
            } else {
                None
            };
            OmodPropertyPlan {
                source: property.clone(),
                target_property,
                unresolved_reason,
            }
        })
        .collect()
}

fn mapped_reference(raw: u32, mapper: &mut FormKeyMapper<'_>) -> Result<u32, String> {
    if raw == 0 {
        return Ok(0);
    }
    let mut bytes = raw.to_le_bytes();
    mapper
        .rewrite_raw_formid_at(&mut bytes, 0)
        .ok_or_else(|| format!("Starfield weapon mod reference {raw:08X} has no target mapping"))?;
    Ok(u32::from_le_bytes(bytes))
}

fn lower_properties(
    source: &[OmodProperty],
    mapper: &mut FormKeyMapper<'_>,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(source.len() * 24);
    for property in source {
        let code = &property.property;
        let target = fo4_property_id(code).ok_or_else(|| {
            format!(
                "Starfield weapon property {} has no proven FO4 instance property",
                String::from_utf8_lossy(code)
            )
        })?;
        let mut value1 = property.value1;
        let mut value2 = property.value2;
        let mut step = property.step;
        let mut value_type = property.value_type;
        if code == b"FTYP" {
            if property.value_type != 5
                || property.function != 0
                || property.value2 != 0
                || property.step != 0.
            {
                return Err("Starfield firing-type property has an unproven representation".into());
            }
            value1 = match value1 {
                0 => 0,
                2 => 1,
                _ => {
                    return Err(
                        "Starfield burst firing has no equivalent FO4 automatic flag".into(),
                    );
                }
            };
            value_type = 2;
        }
        if code == b"FREP" && property.value_type != 2 {
            return Err("Starfield full-auto property requires a boolean".into());
        }
        match value_type {
            3 => {
                return Err(
                    "Starfield weapon string property requires string-table lowering".into(),
                );
            }
            4 | 6 => value1 = mapped_reference(value1, mapper)?,
            2 if value1 > 1 || value2 > 1 => {
                return Err("Starfield weapon mod boolean value is invalid".into());
            }
            5 => {
                let maximum = match code {
                    b"GTYP" => 11,
                    b"WHBV" => 3,
                    b"WSLV" | b"WSTG" => 4,
                    _ => return Err("Starfield weapon mod enum has no verified FO4 mapping".into()),
                };
                if property.function != 0 || value1 > maximum {
                    return Err("Starfield weapon mod enum is outside the FO4 contract".into());
                }
            }
            0..=2 => {}
            _ => return Err("Starfield weapon mod value type is unsupported".into()),
        }
        if matches!(
            code,
            b"WREA" | b"WMNR" | b"WMXR" | b"ZOFX" | b"ZOFY" | b"ZOFZ"
        ) {
            if property.value_type != 1 {
                return Err("Starfield dimensional weapon mod property requires a float".into());
            }
            // MUL+ADD's first operand is dimensionless; only its additive operand changes units.
            if property.function == 1 {
                value2 = (f32::from_bits(value2) * STARFIELD_METERS_TO_FO4_UNITS).to_bits();
            } else {
                value1 = (f32::from_bits(value1) * STARFIELD_METERS_TO_FO4_UNITS).to_bits();
            }
            step *= STARFIELD_METERS_TO_FO4_UNITS;
            if !f32::from_bits(value1).is_finite()
                || !f32::from_bits(value2).is_finite()
                || !step.is_finite()
            {
                return Err("Starfield dimensional weapon property overflows FO4 float".into());
            }
        }
        bytes.extend_from_slice(&[value_type, 0, 0, 0, property.function, 0, 0, 0]);
        bytes.extend_from_slice(&target.to_le_bytes());
        bytes.extend_from_slice(&[0, 0]);
        bytes.extend_from_slice(&value1.to_le_bytes());
        bytes.extend_from_slice(&value2.to_le_bytes());
        bytes.extend_from_slice(&step.to_le_bytes());
    }
    Ok(bytes)
}

fn inherited_properties(
    source: &SourceWeaponOmod,
    mods: &FxHashMap<u32, SourceWeaponOmod>,
    visiting: &mut FxHashSet<u32>,
) -> Result<Vec<OmodProperty>, String> {
    fn append(
        properties: &mut Vec<OmodProperty>,
        incoming: impl IntoIterator<Item = OmodProperty>,
    ) -> Result<(), String> {
        for property in incoming {
            if properties.iter().any(|previous| {
                let overlaps = previous.property == property.property
                    || fo4_property_id(&previous.property)
                        .is_some_and(|target| Some(target) == fo4_property_id(&property.property));
                overlaps && (previous.function != 2 || property.function != 2)
            }) {
                return Err(
                    "Starfield inherited weapon properties require verified override ordering"
                        .into(),
                );
            }
            properties.push(property);
        }
        Ok(())
    }
    let mut properties: Vec<OmodProperty> = Vec::new();
    for include in &source.includes {
        if include.optional
            || include.minimum_level != 0
            || (source.includes.len() > 1 && include.dont_use_all)
        {
            return Err(
                "Starfield conditional weapon mod include requires instance selection".into(),
            );
        }
        if include.raw_mod == 0 || !visiting.insert(include.raw_mod) {
            return Err("Starfield weapon mod include is null or cyclic".into());
        }
        let included = mods.get(&include.raw_mod).ok_or_else(|| {
            format!(
                "Starfield included weapon mod {:08X} is missing",
                include.raw_mod
            )
        })?;
        append(
            &mut properties,
            inherited_properties(included, mods, visiting)?,
        )?;
        visiting.remove(&include.raw_mod);
    }
    append(&mut properties, source.properties.iter().cloned())?;
    Ok(properties)
}

pub(crate) fn lower_source_omod(
    source: &SourceWeaponOmod,
    mods: &FxHashMap<u32, SourceWeaponOmod>,
    mapper: &mut FormKeyMapper<'_>,
) -> Result<Vec<u8>, String> {
    if source.unknown_header.iter().any(|value| *value > 1) || source.unknown_item != 0 {
        return Err("Starfield weapon mod has unproven header or item data".into());
    }
    let properties = inherited_properties(source, mods, &mut FxHashSet::default())?;
    let property_bytes = lower_properties(&properties, mapper)?;
    let mut bytes = Vec::new();
    // Ordinary FO4 OMOD includes do not inherit instance properties. Emit the proven flat closure.
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&(properties.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&source.unknown_header);
    bytes.extend_from_slice(b"WEAP");
    bytes.extend_from_slice(&[source.max_rank, source.level_tier_offset]);
    bytes.extend_from_slice(&mapped_reference(source.attach_point_raw, mapper)?.to_le_bytes());
    bytes.extend_from_slice(&(source.attach_parent_slots_raw.len() as u32).to_le_bytes());
    for raw in &source.attach_parent_slots_raw {
        bytes.extend_from_slice(&mapped_reference(*raw, mapper)?.to_le_bytes());
    }
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&property_bytes);
    Ok(bytes)
}

pub(crate) fn lower_source_object_template(
    source: &SourceObjectTemplate,
    mapper: &mut FormKeyMapper<'_>,
) -> Result<Vec<u8>, String> {
    let level_min = u8::try_from(source.level_min)
        .map_err(|_| "Starfield template minimum level exceeds FO4's u8")?;
    let level_max = u8::try_from(source.level_max)
        .map_err(|_| "Starfield template maximum level exceeds FO4's u8")?;
    if source.parent_combination_index < -1 {
        return Err("Starfield template parent combination index is invalid".into());
    }
    let property_bytes = lower_properties(&source.properties, mapper)?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(source.includes.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&(source.properties.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&[level_min, 0, level_max, 0]);
    bytes.extend_from_slice(&source.parent_combination_index.to_le_bytes());
    bytes.push(u8::from(source.default));
    bytes.extend_from_slice(&(source.keywords_raw.len() as u32).to_le_bytes());
    for raw in &source.keywords_raw {
        bytes.extend_from_slice(&mapped_reference(*raw, mapper)?.to_le_bytes());
    }
    bytes.extend_from_slice(&[source.min_level_for_ranks, source.alt_levels_per_tier]);
    for include in &source.includes {
        if include.raw_mod == 0 {
            return Err("Starfield template mod include is null".into());
        }
        bytes.extend_from_slice(&mapped_reference(include.raw_mod, mapper)?.to_le_bytes());
        bytes.extend_from_slice(&[
            include.attach_point_index,
            u8::from(include.optional),
            u8::from(include.dont_use_all),
        ]);
    }
    bytes.extend_from_slice(&property_bytes);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::ids::FormKey;
    use crate::sym::StringInterner;
    const SHOTTY_MAG: &str = "000000000400000000001B0000005445534F626A656374574541505F496E7374616E636544617461000000D7D4050000000000000000000400000002000000574B4559FA4E370000000000000000000400000002000000574B4559140C2B0000000000000000000400000002000000574B4559130C2B0000000000000000000400000002000000574B4559120C2B000000000000000000";
    const EON_RECEIVER: &str = "010000000400000000001B0000005445534F626A656374574541505F496E7374616E6365446174610000000440020008000000D7D40500992402009C24020028EE020046BC0100C0EC02009F240200FE7A140000000000F74113000000010200000000000000464F52460100000001000000000000000100000000000000575350530000A04000000000000000000400000002000000574B4559FA4E370000000000000000000600000002000000574143564EAD2600000080BF00000000";

    fn test_mapper<'a>(interner: &'a StringInterner, references: &[u32]) -> FormKeyMapper<'a> {
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "Converted.esm".into(),
                source_plugin_name: "Starfield.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                ..MapperOptions::default()
            },
            interner,
        );
        for (index, raw) in references.iter().enumerate() {
            mapper.add_mapping(
                FormKey {
                    plugin: interner.intern("Starfield.esm"),
                    local: *raw,
                },
                FormKey {
                    plugin: interner.intern("Converted.esm"),
                    local: 0x800 + index as u32,
                },
            );
        }
        mapper
    }

    fn uint(bytes: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
    }

    #[test]
    fn source_firing_enum_lowers_to_target_boolean_without_accepting_burst() {
        let interner = StringInterner::new();
        for (source, target) in [(0, 0), (2, 1)] {
            let row = OmodProperty {
                value_type: 5,
                function: 0,
                property: *b"FTYP",
                value1: source,
                value2: 0,
                step: 0.,
            };
            let bytes = lower_properties(&[row], &mut test_mapper(&interner, &[])).unwrap();
            assert_eq!(bytes[0], 2);
            assert_eq!(uint(&bytes, 8), 25);
            assert_eq!(uint(&bytes, 12), target);
        }
        let row = OmodProperty {
            value_type: 5,
            function: 0,
            property: *b"FTYP",
            value1: 1,
            value2: 0,
            step: 0.,
        };
        assert!(
            lower_properties(&[row], &mut test_mapper(&interner, &[]))
                .unwrap_err()
                .contains("burst")
        );
    }

    #[test]
    fn actual_magazine_lowering_encodes_target_ids_and_fo4_property_rows() {
        let source = decode_source_omod(&hex::decode(SHOTTY_MAG).unwrap()).unwrap();
        let interner = StringInterner::new();
        let mut mapper = test_mapper(
            &interner,
            &[0x5D4D7, 0x374EFA, 0x2B0C14, 0x2B0C13, 0x2B0C12],
        );
        let bytes = lower_source_omod(&source, &FxHashMap::default(), &mut mapper).unwrap();
        assert_eq!(&bytes[10..14], b"WEAP");
        assert_eq!(uint(&bytes, 16), 0x01000800);
        assert_eq!(uint(&bytes, 0), 0);
        let layout = crate::fixups::flatten_omod_includes::parse_data_layout(&bytes).unwrap();
        assert_eq!(layout.property_count, 4);
        assert_eq!(layout.properties_end, bytes.len());
        for index in 0..4 {
            let start = layout.properties_start + index * 24;
            assert_eq!(uint(&bytes, start + 8), 31);
            assert_eq!(uint(&bytes, start + 12), 0x01000801 + index as u32);
        }
        let mut missing = test_mapper(&interner, &[]);
        assert!(lower_source_omod(&source, &FxHashMap::default(), &mut missing).is_err());
    }

    #[test]
    fn meter_properties_preserve_mul_operand_and_scale_additive_operand() {
        let mut source = decode_source_omod(&hex::decode(SHOTTY_MAG).unwrap()).unwrap();
        source.attach_point_raw = 0;
        source.properties = vec![OmodProperty {
            value_type: 1,
            function: 1,
            property: *b"WMXR",
            value1: 2_f32.to_bits(),
            value2: 3_f32.to_bits(),
            step: 0.25,
        }];
        let interner = StringInterner::new();
        let mut mapper = test_mapper(&interner, &[]);
        let bytes = lower_source_omod(&source, &FxHashMap::default(), &mut mapper).unwrap();
        assert_eq!(uint(&bytes, 36), 3);
        assert_eq!(f32::from_bits(uint(&bytes, 40)), 2.);
        assert_eq!(
            f32::from_bits(uint(&bytes, 44)),
            3. * STARFIELD_METERS_TO_FO4_UNITS
        );
        assert_eq!(
            f32::from_bits(uint(&bytes, 48)),
            0.25 * STARFIELD_METERS_TO_FO4_UNITS
        );
    }

    #[test]
    fn includes_flatten_only_unconditional_unambiguous_properties() {
        let mut source = decode_source_omod(&hex::decode(SHOTTY_MAG).unwrap()).unwrap();
        source.attach_point_raw = 0;
        source.properties.clear();
        let mut parent = source.clone();
        parent.properties.push(OmodProperty {
            value_type: 2,
            function: 0,
            property: *b"WNDP",
            value1: 1,
            value2: 0,
            step: 0.,
        });
        source.includes.push(OmodInclude {
            raw_mod: 0x123,
            minimum_level: 0,
            optional: false,
            dont_use_all: true,
        });
        let mut mods = FxHashMap::default();
        mods.insert(0x123, parent);
        let interner = StringInterner::new();
        let mut mapper = test_mapper(&interner, &[]);
        let bytes = lower_source_omod(&source, &mods, &mut mapper).unwrap();
        assert_eq!(uint(&bytes, 0), 0);
        assert_eq!(uint(&bytes, 4), 1);
        assert_eq!(uint(&bytes, 36), 26);
        source.includes[0].optional = true;
        assert!(lower_source_omod(&source, &mods, &mut mapper).is_err());
        source.includes[0].optional = false;
        source.properties = mods[&0x123].properties.clone();
        assert!(lower_source_omod(&source, &mods, &mut mapper).is_err());
        source.properties.clear();
        mods.get_mut(&0x123).unwrap().includes = source.includes.clone();
        assert!(lower_source_omod(&source, &mods, &mut mapper).is_err());
    }

    #[test]
    fn sibling_include_overrides_and_different_codes_for_same_target_field_are_rejected() {
        let mut root = decode_source_omod(&hex::decode(SHOTTY_MAG).unwrap()).unwrap();
        root.properties.clear();
        root.includes = [0x123, 0x456]
            .into_iter()
            .map(|raw_mod| OmodInclude {
                raw_mod,
                minimum_level: 0,
                optional: false,
                dont_use_all: false,
            })
            .collect();
        let mut first = root.clone();
        first.includes.clear();
        first.properties = vec![OmodProperty {
            value_type: 2,
            function: 0,
            property: *b"WNDP",
            value1: 0,
            value2: 0,
            step: 0.,
        }];
        let mut second = first.clone();
        second.properties[0].value1 = 1;
        let mut mods = FxHashMap::from_iter([(0x123, first), (0x456, second)]);
        assert!(
            inherited_properties(&root, &mods, &mut FxHashSet::default())
                .unwrap_err()
                .contains("ordering")
        );
        mods.get_mut(&0x123).unwrap().properties[0].property = *b"FREP";
        let property = &mut mods.get_mut(&0x456).unwrap().properties[0];
        property.property = *b"FTYP";
        property.value_type = 5;
        property.value1 = 2;
        assert!(
            inherited_properties(&root, &mods, &mut FxHashSet::default())
                .unwrap_err()
                .contains("ordering")
        );
        for item in mods.values_mut() {
            item.properties[0].property = *b"WKEY";
            item.properties[0].value_type = 4;
            item.properties[0].function = 2;
        }
        assert_eq!(
            inherited_properties(&root, &mods, &mut FxHashSet::default())
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn templates_rewrite_mods_and_keywords_without_wrapping_levels() {
        let source = SourceObjectTemplate {
            level_min: 2,
            level_max: 200,
            parent_combination_index: -1,
            default: true,
            keywords_raw: vec![0x456],
            min_level_for_ranks: 3,
            alt_levels_per_tier: 4,
            includes: vec![TemplateInclude {
                raw_mod: 0x123,
                attach_point_index: 7,
                optional: true,
                dont_use_all: false,
            }],
            properties: vec![],
        };
        let interner = StringInterner::new();
        let mut mapper = test_mapper(&interner, &[0x456, 0x123]);
        let bytes = lower_source_object_template(&source, &mut mapper).unwrap();
        assert_eq!(&bytes[8..12], &[2, 0, 200, 0]);
        assert_eq!(uint(&bytes, 19), 0x01000800);
        assert_eq!(uint(&bytes, 25), 0x01000801);
        assert_eq!(&bytes[29..], &[7, 1, 0]);
        let mut high = source;
        high.level_max = 256;
        assert!(lower_source_object_template(&high, &mut mapper).is_err());
    }

    #[test]
    fn templates_preserve_include_roles_and_reject_truncation() {
        let mut bytes = vec![0; 21];
        bytes[..4].copy_from_slice(&1_u32.to_le_bytes());
        bytes[12..14].copy_from_slice(&(-1_i16).to_le_bytes());
        bytes[14] = 1;
        bytes.extend_from_slice(&0x88BCD_u32.to_le_bytes());
        bytes.extend_from_slice(&[7, 1, 0]);
        let template = decode_source_object_template(&bytes).unwrap();
        assert!(template.default);
        assert_eq!(template.parent_combination_index, -1);
        assert_eq!(template.includes[0].raw_mod, 0x88BCD);
        assert_eq!(template.includes[0].attach_point_index, 7);
        assert!(template.includes[0].optional);
        for end in [0, 13, 20, bytes.len() - 1] {
            assert!(decode_source_object_template(&bytes[..end]).is_err());
        }
        bytes[27] = 2;
        assert!(decode_source_object_template(&bytes).is_err());
    }

    #[test]
    fn actual_shotty_magazine_retains_three_moving_node_remap_keywords() {
        let source = decode_source_omod(&hex::decode(SHOTTY_MAG).unwrap()).unwrap();
        assert_eq!(source.attach_point_raw, 0x5D4D7);
        assert_eq!(
            source
                .properties
                .iter()
                .map(|p| p.value1)
                .collect::<Vec<_>>(),
            [0x374EFA, 0x2B0C14, 0x2B0C13, 0x2B0C12]
        );
        assert!(
            property_plan(&source)
                .iter()
                .all(|p| p.target_property == Some(31) && p.unresolved_reason.is_none())
        );
    }

    #[test]
    fn actual_receiver_preserves_includes_and_marks_rate_contract_unresolved() {
        let source = decode_source_omod(&hex::decode(EON_RECEIVER).unwrap()).unwrap();
        assert_eq!(source.includes[0].raw_mod, 0x1341F7);
        assert_eq!(source.attach_parent_slots_raw.len(), 8);
        assert_eq!(source.properties[1].property, *b"WSPS");
        assert_eq!(f32::from_bits(source.properties[1].value1), 5.);
        let plan = property_plan(&source);
        assert!(plan[0].unresolved_reason.as_ref().unwrap().contains("FORF"));
        assert!(plan[1].unresolved_reason.as_ref().unwrap().contains("WSPS"));
        assert_eq!(plan[3].target_property, Some(94));
    }

    #[test]
    fn corrupt_modular_counts_and_truncation_are_rejected() {
        let bytes = hex::decode(SHOTTY_MAG).unwrap();
        for end in [0, 9, 13, 50, bytes.len() - 1] {
            assert!(decode_source_omod(&bytes[..end]).is_err());
        }
        let mut corrupt = bytes.clone();
        corrupt[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_source_omod(&corrupt).is_err());
        let mut extra = bytes;
        extra.push(0);
        assert!(decode_source_omod(&extra).is_err());
    }
}
