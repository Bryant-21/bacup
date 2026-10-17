use super::*;

#[test]
fn skyrim_weapon_layout_failures_do_not_emit_source_bytes() {
    let mut bad_ammo = subrecord(b"EDID", b"BadAmmo\0");
    bad_ammo.extend_from_slice(&subrecord(b"DATA", &[0; 15]));
    let mut bad_projectile = subrecord(b"EDID", b"BadAmmoOwner\0");
    let mut data = [0_u8; 20];
    data[..4].copy_from_slice(&0x0200_0800_u32.to_le_bytes());
    bad_projectile.extend_from_slice(&subrecord(b"DATA", &data));
    let mut bad_weapon = subrecord(b"EDID", b"BadWeapon\0");
    bad_weapon.extend_from_slice(&subrecord(b"DATA", &[0; 10]));
    bad_weapon.extend_from_slice(&subrecord(b"DNAM", &[0; 99]));
    let temp = tempfile::tempdir().unwrap();
    let source_path = temp.path().join("Skyrim.esm");
    std::fs::write(
        &source_path,
        plugin(
            &[],
            &[
                group(
                    b"AMMO",
                    0,
                    &[
                        record(b"AMMO", 0x800, 0, &bad_ammo),
                        record(b"AMMO", 0x801, 0, &bad_projectile),
                    ],
                ),
                group(b"WEAP", 0, &[record(b"WEAP", 0x802, 0, &bad_weapon)]),
            ],
        ),
    )
    .unwrap();
    let source_handle = load_source_handle(&source_path, "skyrimse");
    let target_handle = fresh_target_handle();
    let id = create_run(RunParams {
        source: Game::SkyrimSe,
        target: Game::Fo4,
        source_handle_id: source_handle,
        target_handle_id: target_handle,
        master_handle_ids: vec![],
        config: RunConfig {
            output_plugin_name: "Out.esm".into(),
            preserve_source_ids: true,
            ..Default::default()
        },
    })
    .unwrap();
    let stats = with_run(id, |run| run.translate_all_v2(&source_path)).unwrap();
    assert_eq!(stats.records_failed, 3);
    assert_eq!(stats.records_translated, 0);
    assert!(handle_records(target_handle).is_empty());
    drop_run(id).unwrap();
    plugin_handle_close_native(source_handle);
    plugin_handle_close_native(target_handle);
}
