use crate::record::{FieldValue, Record};

const MODEL_EXTENSIONS: &[&str] = &[".nif", ".hkx", ".kf", ".egm", ".egt"];
const KNOWN_ASSET_PREFIXES: &[&str] = &[
    "fo76",
    "fnv",
    "fo3",
    "fo4",
    "skyrim",
    "skyrimse",
    "starfield",
    "oblivion",
];

pub(super) fn normalize_model_paths(interner: &crate::sym::StringInterner, record: &mut Record) {
    for field in record.fields.iter_mut() {
        normalize_model_path_value(interner, &mut field.value);
    }
}

fn normalize_model_path_value(interner: &crate::sym::StringInterner, value: &mut FieldValue) {
    match value {
        FieldValue::String(sym) => {
            let Some(path) = interner.resolve(*sym) else {
                return;
            };
            if let Some(normalized) = normalized_model_path(path) {
                *sym = interner.intern(&normalized);
            }
        }
        FieldValue::List(items) => {
            for item in items {
                normalize_model_path_value(interner, item);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, field_value) in fields {
                normalize_model_path_value(interner, field_value);
            }
        }
        _ => {}
    }
}

fn normalized_model_path(path: &str) -> Option<String> {
    let mut normalized = path.trim().trim_matches('\0').replace('\\', "/");
    normalized = normalized.trim_start_matches('/').to_string();
    if normalized.is_empty()
        || normalized.eq_ignore_ascii_case("none")
        || normalized.contains(':')
        || normalized.starts_with("0x")
        || normalized.starts_with("0X")
    {
        return None;
    }

    let lower = normalized.to_ascii_lowercase();
    if !MODEL_EXTENSIONS.iter().any(|ext| lower.ends_with(ext)) {
        return None;
    }

    if normalized.len() >= 5 && normalized[..5].eq_ignore_ascii_case("data/") {
        normalized = normalized[5..].to_string();
    }
    if normalized.len() >= 7 && normalized[..7].eq_ignore_ascii_case("meshes/") {
        normalized = normalized[7..].to_string();
    }

    let first_component = normalized.split('/').next().unwrap_or_default();
    if KNOWN_ASSET_PREFIXES
        .iter()
        .any(|prefix| first_component.eq_ignore_ascii_case(prefix))
    {
        normalized = normalized
            .split_once('/')
            .map(|(_, rest)| rest.to_string())
            .unwrap_or_default();
    }

    let output = normalized.replace('/', "\\");
    if output == path.trim().trim_matches('\0') {
        return None;
    }
    Some(output)
}
