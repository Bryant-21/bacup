use conversion_native::formkey_mapper::{FormKeyMapper, MapperOptions};
use conversion_native::ids::{FormKey, SigCode};
use conversion_native::sym::StringInterner;

#[test]
fn generated_reservations_block_fresh_allocations_but_allow_source_id_preservation() {
    let interner = StringInterner::new();
    let source_plugin = interner.intern("Source.esm");
    let mut mapper = FormKeyMapper::new(
        [],
        MapperOptions {
            output_plugin_name: "Mod.esp".to_string(),
            preserve_source_ids: true,
            generated_object_id_floor: 0x00A0_0000,
            ..Default::default()
        },
        &interner,
    );
    mapper.reserve_generated_object_ids([0x000900, 0x00A0_0000]);

    let preserved = mapper.allocate_or_resolve(
        FormKey {
            local: 0x000900,
            plugin: source_plugin,
        },
        None,
        SigCode::from_str("WEAP").unwrap(),
    );
    let generated = mapper.allocate_generated();

    assert_eq!(preserved.local, 0x000900);
    assert_eq!(generated.local, 0x00A0_0001);
}
