#[test]
fn post_translate_replaces_legacy_speedtree_models_with_fo4_nifs() {
    let cases = [
        (
            "/EuonymusBush01.spt",
            "Landscape\\Plants\\ShrubGroupSmall01.nif",
        ),
        (
            "/WastelandShrub01.spt",
            "Landscape\\Plants\\DeadShrub01.nif",
        ),
        (
            "/WastelandUndergrowth01.spt",
            "Landscape\\Plants\\Bramble01.nif",
        ),
        ("/OasisElm01.spt", "Landscape\\Trees\\TreeMapleForest1.nif"),
        ("/OasisElm02.spt", "Landscape\\Trees\\TreeMapleForest2.nif"),
        ("/Pine01.spt", "Landscape\\Trees\\TreeMapleForest3.nif"),
        (
            "/SugarMaple01.spt",
            "Landscape\\Trees\\TreeMapleForest4.nif",
        ),
        ("/Sycamore01.spt", "Landscape\\Trees\\TreeMapleForest5.nif"),
        ("/WhiteOak01.spt", "Landscape\\Trees\\TreeMapleForest6.nif"),
        (
            "/OasisTreeTop01.spt",
            "Landscape\\Trees\\TreeMapleForestsmall1.nif",
        ),
    ];

    for (source, expected) in cases {
        let interner = StringInterner::new();
        let mut record = make_record("TREE", &interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern(source)),
        );

        FnvFo4Hook
            .post_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let FieldValue::String(model) = record.fields[0].value else {
            panic!("TREE MODL must remain a string");
        };
        assert_eq!(interner.resolve(model), Some(expected));
    }
}

#[test]
fn fo3_post_translate_replaces_shared_legacy_speedtree_model() {
    let interner = StringInterner::new();
    let mut record = make_record("TREE", &interner);
    push_field(
        &mut record,
        "MODL",
        FieldValue::String(interner.intern("Trees\\WhiteOak01.SPT")),
    );

    Fo3Fo4Hook
        .post_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    let FieldValue::String(model) = record.fields[0].value else {
        panic!("TREE MODL must remain a string");
    };
    assert_eq!(
        interner.resolve(model),
        Some("Landscape\\Trees\\TreeMapleForest6.nif")
    );
}

#[test]
fn post_translate_leaves_nif_tree_models_unchanged() {
    let interner = StringInterner::new();
    let mut record = make_record("TREE", &interner);
    let source = "Landscape\\Trees\\CustomTree.nif";
    push_field(
        &mut record,
        "MODL",
        FieldValue::String(interner.intern(source)),
    );

    FnvFo4Hook
        .post_translate(&mut make_ctx(&interner), &mut record)
        .unwrap();

    let FieldValue::String(model) = record.fields[0].value else {
        panic!("TREE MODL must remain a string");
    };
    assert_eq!(interner.resolve(model), Some(source));
}
