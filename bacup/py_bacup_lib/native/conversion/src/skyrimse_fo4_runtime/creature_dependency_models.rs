use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

pub(crate) fn model_paths(
    record: &Record,
    interner: &StringInterner,
) -> Result<Vec<String>, String> {
    let mut paths = Vec::new();
    for field in &record.fields {
        let value = match (record.sig.as_str(), field.sig.as_str(), &field.value) {
            ("DEBR", "DATA", FieldValue::Struct(values)) => values
                .iter()
                .find(|(name, _)| interner.resolve(*name) == Some("model_file_name"))
                .map(|(_, value)| value),
            ("DEBR", "DATA", _) => {
                return Err(
                    "DEBR.DATA model must be decoded before creature dependency staging".into(),
                );
            }
            ("EXPL" | "ARTO", "MODL", value) => Some(value),
            _ => None,
        };
        let Some(value) = value else { continue };
        let FieldValue::String(value) = value else {
            return Err(format!(
                "{}.{} model path is not a string",
                record.sig.as_str(),
                field.sig.as_str()
            ));
        };
        let path = interner
            .resolve(*value)
            .ok_or("unresolved model path")?
            .trim()
            .replace('\\', "/")
            .to_ascii_lowercase();
        if path.is_empty() {
            continue;
        }
        if !path.ends_with(".nif")
            || path.starts_with('/')
            || path
                .split('/')
                .any(|part| part == ".." || part.contains(':'))
        {
            return Err(format!("invalid creature dependency model path {path:?}"));
        }
        paths.push(if path.starts_with("meshes/") {
            path
        } else {
            format!("meshes/{path}")
        });
    }
    Ok(paths)
}

pub(crate) fn paired_model_paths(
    source: &Record,
    target: &Record,
    interner: &StringInterner,
) -> Result<Vec<(String, String)>, String> {
    let source_paths = model_paths(source, interner)?;
    let target_paths = model_paths(target, interner)?;
    if source.sig != target.sig || source_paths.len() != target_paths.len() {
        return Err(format!(
            "{} dependency model closure changed during record translation",
            source.sig.as_str()
        ));
    }
    Ok(source_paths.into_iter().zip(target_paths).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    #[test]
    fn debris_models_preserve_order_and_use_translated_runtime_paths() {
        let interner = StringInterner::new();
        let make = |plugin: &str, paths: &[&str]| {
            let mut record = Record::new(
                SigCode(*b"DEBR"),
                FormKey::parse(&format!("000800@{plugin}"), &interner).unwrap(),
            );
            for path in paths {
                record.fields.push(FieldEntry {
                    sig: SubrecordSig(*b"DATA"),
                    value: FieldValue::Struct(vec![(
                        interner.intern("model_file_name"),
                        FieldValue::String(interner.intern(path)),
                    )]),
                });
            }
            record
        };
        let source = make("Skyrim.esm", &["Effects\\Ice.nif", "Effects\\Rock.nif"]);
        let target = make(
            "Output.esm",
            &["Skyrim\\Effects\\Ice.nif", "Effects\\Rock.nif"],
        );
        assert_eq!(
            paired_model_paths(&source, &target, &interner).unwrap(),
            [
                (
                    "meshes/effects/ice.nif".into(),
                    "meshes/skyrim/effects/ice.nif".into()
                ),
                (
                    "meshes/effects/rock.nif".into(),
                    "meshes/effects/rock.nif".into()
                )
            ]
        );
        assert!(
            paired_model_paths(
                &source,
                &make("Output.esm", &["Effects\\Ice.nif"]),
                &interner
            )
            .is_err()
        );
    }
}
