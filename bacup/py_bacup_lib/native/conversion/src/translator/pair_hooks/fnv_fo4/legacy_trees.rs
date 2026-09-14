use crate::record::{FieldValue, Record};

const LEGACY_TREE_MODELS: &[(&str, &str)] = &[
    (
        "euonymusbush01.spt",
        "Landscape\\Plants\\ShrubGroupSmall01.nif",
    ),
    ("wastelandshrub01.spt", "Landscape\\Plants\\DeadShrub01.nif"),
    (
        "wastelandundergrowth01.spt",
        "Landscape\\Plants\\Bramble01.nif",
    ),
    ("oasiselm01.spt", "Landscape\\Trees\\TreeMapleForest1.nif"),
    ("oasiselm02.spt", "Landscape\\Trees\\TreeMapleForest2.nif"),
    ("pine01.spt", "Landscape\\Trees\\TreeMapleForest3.nif"),
    ("sugarmaple01.spt", "Landscape\\Trees\\TreeMapleForest4.nif"),
    ("sycamore01.spt", "Landscape\\Trees\\TreeMapleForest5.nif"),
    ("whiteoak01.spt", "Landscape\\Trees\\TreeMapleForest6.nif"),
    (
        "oasistreetop01.spt",
        "Landscape\\Trees\\TreeMapleForestsmall1.nif",
    ),
];

pub(super) fn rewrite_legacy_speedtree_model(
    interner: &crate::sym::StringInterner,
    record: &mut Record,
) {
    if record.sig.0 != *b"TREE" {
        return;
    }

    for field in &mut record.fields {
        if field.sig.0 != *b"MODL" {
            continue;
        }
        let FieldValue::String(model) = &field.value else {
            continue;
        };
        let Some(path) = interner.resolve(*model) else {
            continue;
        };
        let Some(replacement) = legacy_tree_replacement(path) else {
            continue;
        };
        field.value = FieldValue::String(interner.intern(replacement));
    }
}

fn legacy_tree_replacement(path: &str) -> Option<&'static str> {
    let filename = path
        .trim()
        .trim_matches('\0')
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default();
    LEGACY_TREE_MODELS
        .iter()
        .find_map(|(source, target)| filename.eq_ignore_ascii_case(source).then_some(*target))
}
