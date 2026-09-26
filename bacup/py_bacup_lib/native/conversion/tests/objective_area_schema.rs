use esp_authoring_core::plugin_runtime::{
    ParsedSubrecord, compiled_schema_for_game_str, iter_referenced_form_ids_from_subrecords,
    rewrite_referenced_form_ids_in_subrecords,
};

#[test]
fn objective_area_schema_selects_and_remaps_only_global_radius() {
    let schema = compiled_schema_for_game_str("fo76").unwrap();
    for flags in [0_u8, 1, 2, 3, 128, 129, 130, 131] {
        let global = flags & 1 != 0;
        let mut data = vec![0; 14];
        data[5] = flags;
        data[6..10].copy_from_slice(&0x800_u32.to_le_bytes());
        data[10..14].copy_from_slice(&0x801_u32.to_le_bytes());
        let mut fields = vec![ParsedSubrecord {
            signature: "QSTA".into(),
            data: data.into(),
            semantic_type: None,
        }];
        let refs = iter_referenced_form_ids_from_subrecords("QUST", &fields, Some(&schema));
        assert!(refs.iter().any(|(_, raw)| *raw == 0x800));
        assert_eq!(refs.iter().any(|(_, raw)| *raw == 0x801), global);
        assert!(rewrite_referenced_form_ids_in_subrecords(
            "QUST",
            &mut fields,
            Some(&schema),
            &mut |raw| Some(raw + 0x100)
        ));
        assert_eq!(
            u32::from_le_bytes(fields[0].data[6..10].try_into().unwrap()),
            0x900
        );
        assert_eq!(
            u32::from_le_bytes(fields[0].data[10..14].try_into().unwrap()),
            if global { 0x901 } else { 0x801 }
        );
    }
}
