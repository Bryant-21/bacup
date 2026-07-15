//! Skyrim whole-plugin NAVM preparation for the store2 translation path.

use std::collections::HashMap;

use esp_authoring_core::nvnm::convert_skyrim_nvnm_set_to_fo4_lossy;
use smallvec::SmallVec;

use crate::record::{FieldValue, Record};
use crate::store2::source::SourceEsm;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SkyrimNavmeshPrepareReport {
    pub records_seen: usize,
    pub records_converted: usize,
    pub records_failed: usize,
    pub records_without_geometry: usize,
    pub edge_links_resolved: usize,
    pub edge_links_dropped: usize,
    pub cover_triangles_dropped: usize,
}

pub(crate) struct PreparedSkyrimNavmeshes {
    pub converted: HashMap<u32, Vec<u8>>,
    pub failures: HashMap<u32, String>,
    pub report: SkyrimNavmeshPrepareReport,
}

pub(crate) fn prepare_skyrim_navmeshes(esm: &SourceEsm) -> PreparedSkyrimNavmeshes {
    let mut source_payloads = Vec::new();
    let mut failures = HashMap::new();
    let mut report = SkyrimNavmeshPrepareReport::default();
    for position in esm.positions_of_sig(*b"NAVM") {
        report.records_seen += 1;
        let Some(view) = esm.view_at(position) else {
            report.records_failed += 1;
            continue;
        };
        let record = match view.to_parsed_record() {
            Ok(record) => record,
            Err(error) => {
                report.records_failed += 1;
                failures.insert(view.form_id(), format!("record decode failed: {error}"));
                continue;
            }
        };
        let geometry = record
            .subrecords
            .iter()
            .find(|subrecord| subrecord.signature.as_str() == "NVNM")
            .map(|subrecord| subrecord.data.clone());
        match geometry {
            Some(bytes) if !bytes.is_empty() => source_payloads.push((view.form_id(), bytes)),
            _ => {
                report.records_without_geometry += 1;
                report.records_failed += 1;
                failures.insert(view.form_id(), "missing NVNM geometry".to_string());
            }
        }
    }

    let borrowed = source_payloads
        .iter()
        .map(|(form_id, bytes)| (*form_id, bytes.as_ref()))
        .collect::<Vec<_>>();
    let batch = convert_skyrim_nvnm_set_to_fo4_lossy(&borrowed);
    report.records_failed += batch.failures.len();
    for failure in batch.failures {
        failures.insert(failure.form_id, failure.error.to_string());
    }
    let mut by_form_id = HashMap::with_capacity(batch.converted.len());
    for conversion in batch.converted {
        report.records_converted += 1;
        report.edge_links_resolved += conversion.report.edge_links_resolved;
        report.edge_links_dropped += conversion.report.edge_links_dropped;
        report.cover_triangles_dropped += conversion.report.cover_triangles_dropped;
        by_form_id.insert(conversion.form_id, conversion.bytes);
    }
    PreparedSkyrimNavmeshes {
        converted: by_form_id,
        failures,
        report,
    }
}

pub(crate) fn install_converted_nvnm(record: &mut Record, bytes: &[u8]) -> Result<(), String> {
    let Some(field) = record
        .fields
        .iter_mut()
        .find(|field| field.sig.as_str() == "NVNM")
    else {
        return Err("translated Skyrim NAVM has no NVNM field".to_string());
    };
    let FieldValue::Bytes(value) = &mut field.value else {
        return Err("translated Skyrim NAVM NVNM field is not raw bytes".to_string());
    };
    *value = SmallVec::from_slice(bytes);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::FieldEntry;
    use crate::store2::source::test_fixture::{group, plugin, record, subrecord};
    use crate::sym::StringInterner;
    use esp_authoring_core::nvnm::parse_nvnm;
    use std::io::Write;

    #[test]
    fn prepares_real_skyrim_navm_from_store2_source() {
        let source_nvnm = hex::decode(
            include_str!("../../../../../py_creation_lib/native/esp/src/nvnm/tests/fixtures/0e537d_skyrim_v12.nvnm.hex").trim(),
        )
        .unwrap();
        let navm = record(b"NAVM", 0x000E_537D, 0, &subrecord(b"NVNM", &source_nvnm));
        let bytes = plugin(&[], &[group(b"NAVM", 0, &[navm])]);
        let mut file = tempfile::Builder::new().suffix(".esm").tempfile().unwrap();
        file.write_all(&bytes).unwrap();
        file.flush().unwrap();
        let esm = SourceEsm::open(file.path()).unwrap();

        let prepared = prepare_skyrim_navmeshes(&esm);

        assert_eq!(prepared.report.records_seen, 1);
        assert_eq!(prepared.report.records_converted, 1);
        assert_eq!(prepared.report.records_failed, 0);
        assert_eq!(prepared.report.records_without_geometry, 0);
        let parsed = parse_nvnm(prepared.converted.get(&0x000E_537D).unwrap()).unwrap();
        assert_eq!(parsed.version, 15);
        assert_eq!(parsed.vertices.len(), 95);
        assert_eq!(parsed.triangles.len(), 98);
        assert_eq!(parsed.grid.divisor, 3);
    }

    #[test]
    fn malformed_navm_does_not_abort_valid_store2_preparation() {
        let valid = hex::decode(
            include_str!("../../../../../py_creation_lib/native/esp/src/nvnm/tests/fixtures/0e537d_skyrim_v12.nvnm.hex").trim(),
        )
        .unwrap();
        let malformed = 12u32.to_le_bytes();
        let valid_record = record(b"NAVM", 0x000E_537D, 0, &subrecord(b"NVNM", &valid));
        let bad_record = record(b"NAVM", 0x000E_537E, 0, &subrecord(b"NVNM", &malformed));
        let bytes = plugin(&[], &[group(b"NAVM", 0, &[valid_record, bad_record])]);
        let mut file = tempfile::Builder::new().suffix(".esm").tempfile().unwrap();
        file.write_all(&bytes).unwrap();
        file.flush().unwrap();
        let esm = SourceEsm::open(file.path()).unwrap();

        let prepared = prepare_skyrim_navmeshes(&esm);

        assert_eq!(prepared.report.records_seen, 2);
        assert_eq!(prepared.report.records_converted, 1);
        assert_eq!(prepared.report.records_failed, 1);
        assert!(prepared.converted.contains_key(&0x000E_537D));
        assert!(prepared.failures.contains_key(&0x000E_537E));
    }

    #[test]
    fn installs_v15_geometry_in_translated_navm() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("NAVM").unwrap(),
            FormKey::parse("000800@Skyrim_Merged.esm", &interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("NVNM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(&12u32.to_le_bytes())),
        });

        install_converted_nvnm(&mut record, &15u32.to_le_bytes()).unwrap();

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("NVNM should remain raw bytes");
        };
        assert_eq!(bytes.as_slice(), &15u32.to_le_bytes());
    }
}
