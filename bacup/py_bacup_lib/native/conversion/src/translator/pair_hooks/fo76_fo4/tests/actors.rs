
    #[test]
    fn pre_translate_strips_only_bee_swarm_ant_limb_replacements() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let mut bee_parts = Record::new(
            SigCode::from_str("BPTD").unwrap(),
            FormKey {
                local: HONEY_BEAST_BEE_SWARM_BPTD_FORM_ID,
                plugin,
            },
        );
        bee_parts.eid = Some(interner.intern(HONEY_BEAST_BEE_SWARM_BPTD_EDITOR_ID));
        push_field(
            &mut bee_parts,
            "NAM1",
            FieldValue::String(
                interner.intern("Actors\\DLC04\\Swarm\\CharacterAssets\\ReplaceSwarm01.nif"),
            ),
        );
        push_field(&mut bee_parts, "BPND", raw_bytes(&[0; 112]));
        push_field(
            &mut bee_parts,
            "NAM1",
            FieldValue::String(
                interner.intern("Actors\\DLC04\\Swarm\\CharacterAssets\\ReplaceSwarm02.nif"),
            ),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut bee_parts)
            .unwrap();

        assert!(bee_parts.fields.iter().all(|field| field.sig.0 != *b"NAM1"));
        assert!(bee_parts.fields.iter().any(|field| field.sig.0 == *b"BPND"));

        let mut other_parts = make_record("BPTD", &interner);
        other_parts.eid = Some(interner.intern("OtherCreatureBodyPartData"));
        push_field(
            &mut other_parts,
            "NAM1",
            FieldValue::String(interner.intern("Actors\\Other\\Replacement.nif")),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut other_parts)
            .unwrap();

        assert!(
            other_parts
                .fields
                .iter()
                .any(|field| field.sig.0 == *b"NAM1")
        );
    }

    #[test]
    fn pre_translate_normalizes_biped_slot_masks() {
        let interner = StringInterner::new();
        let full_body_child_mask = FO4_BODY_BIPED_MASK | FO76_UPPER_BODY_SKIN_BIPED_MASK;
        for (name, sig, editor_id, child_race, mask, expected) in [
            (
                "chinese_stealth_keeps_pipboy_visible",
                "ARMA",
                Some("AA_ArmorChineseStealth"),
                false,
                (1 << (33 - 30)) | (1 << (60 - 30)),
                1 << (33 - 30),
            ),
            (
                "child_underarmor_maps_to_body",
                "ARMO",
                None,
                true,
                FO76_UPPER_BODY_SKIN_BIPED_MASK,
                FO4_BODY_BIPED_MASK,
            ),
            (
                "full_body_child_armor_kept",
                "ARMO",
                None,
                true,
                full_body_child_mask,
                full_body_child_mask,
            ),
        ] {
            let mut record = make_record(sig, &interner);
            if let Some(editor_id) = editor_id {
                push_field(
                    &mut record,
                    "EDID",
                    FieldValue::String(interner.intern(editor_id)),
                );
            }
            if child_race {
                push_field(
                    &mut record,
                    "RNAM",
                    FieldValue::FormKey(FormKey {
                        local: FO4_HUMAN_CHILD_RACE_FORM_ID,
                        plugin: interner.intern("SeventySix.esm"),
                    }),
                );
            }
            push_field(&mut record, "BOD2", FieldValue::Uint(mask));

            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            let mask = record
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"BOD2")
                .and_then(|entry| match entry.value {
                    FieldValue::Uint(mask) => Some(mask),
                    _ => None,
                })
                .expect("BOD2 mask");
            assert_eq!(mask, expected, "{name}");
        }
    }

    fn raw_ctda(function_id: u16) -> FieldValue {
        raw_ctda_with_parameter_1(function_id, 0)
    }

    fn raw_ctda_with_parameter_1(function_id: u16, parameter_1: u32) -> FieldValue {
        let mut bytes = vec![0_u8; 32];
        bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        bytes[12..16].copy_from_slice(&parameter_1.to_le_bytes());
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    fn raw_ctda_with_run_on(function_id: u16, parameter_1: u32, run_on: u32) -> FieldValue {
        let mut bytes = vec![0_u8; 32];
        bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        bytes[12..16].copy_from_slice(&parameter_1.to_le_bytes());
        bytes[20..24].copy_from_slice(&run_on.to_le_bytes());
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    fn raw_fo76_destruction_stage(health: u8, flags: u8, explosion: u32) -> FieldValue {
        let mut bytes = vec![0_u8; 28];
        bytes[0] = health;
        bytes[3] = flags;
        bytes[8..12].copy_from_slice(&explosion.to_le_bytes());
        raw_bytes(&bytes)
    }

    #[test]
    fn pre_translate_normalizes_only_explosive_mstt_destruction_flags() {
        let interner = StringInterner::new();
        for (name, stages, expected) in [
            (
                "explosive_vehicle",
                vec![(85, 0, 0x0022_496), (50, 0x05, 0x0023_D3B), (0, 0x08, 0)],
                vec![0, 0x04, 0],
            ),
            ("non_explosive", vec![(50, 0x05, 0), (0, 0x08, 0)], vec![0x05, 0x08]),
        ] {
            let mut record = make_record("MSTT", &interner);
            for (health, flags, explosion) in stages {
                push_field(
                    &mut record,
                    "DSTD",
                    raw_fo76_destruction_stage(health, flags, explosion),
                );
            }

            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            let flags: Vec<u8> = record
                .fields
                .iter()
                .filter(|entry| entry.sig.0 == *b"DSTD")
                .map(|entry| match &entry.value {
                    FieldValue::Bytes(bytes) => bytes[3],
                    _ => panic!("DSTD should remain raw before struct relayout"),
                })
                .collect();
            assert_eq!(flags, expected, "{name}");
        }
    }

    fn read_vmad_string(bytes: &[u8], offset: &mut usize) -> String {
        let length = u16::from_le_bytes(bytes[*offset..*offset + 2].try_into().unwrap()) as usize;
        *offset += 2;
        let value = std::str::from_utf8(&bytes[*offset..*offset + length])
            .unwrap()
            .to_string();
        *offset += length;
        value
    }

    fn read_power_armor_vmad(bytes: &[u8]) -> (String, Vec<(String, u32)>) {
        let mut offset = 0;
        assert_eq!(
            u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()),
            FO4_VMAD_VERSION
        );
        offset += 2;
        assert_eq!(
            u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()),
            FO4_VMAD_OBJECT_FORMAT
        );
        offset += 2;
        assert_eq!(
            u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()),
            1
        );
        offset += 2;

        let script_name = read_vmad_string(bytes, &mut offset);
        assert_eq!(bytes[offset], 0);
        offset += 1;
        let property_count =
            u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()) as usize;
        offset += 2;

        let mut properties = Vec::with_capacity(property_count);
        for _ in 0..property_count {
            let name = read_vmad_string(bytes, &mut offset);
            assert_eq!(bytes[offset], 1);
            offset += 1;
            assert_eq!(bytes[offset], VMAD_PROPERTY_FLAG_EDITED);
            offset += 1;
            assert_eq!(
                u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()),
                0
            );
            offset += 2;
            assert_eq!(
                i16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap()),
                -1
            );
            offset += 2;
            let form_id = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            offset += 4;
            properties.push((name, form_id));
        }
        assert_eq!(offset, bytes.len());
        (script_name, properties)
    }

    #[test]
    fn pre_translate_typed_refs_participate_in_formkey_mapper() {
        let interner = StringInterner::new();
        for (name, sig, field, input, subfield, local, target_plugin, output_plugin) in [
            ("note_snam", "NOTE", "SNAM", FieldValue::Uint(0x0053_4F51), None, 0x534F51, "Output.esm", "Output.esm"),
            ("npc_snam", "NPC_", "SNAM", raw_bytes(&[0x04, 0x83, 0x05, 0x00, 0x00]), Some("faction"), 0x058304, "Fallout4.esm", "SeventySix.esm"),
            ("npc_cnto", "NPC_", "CNTO", raw_bytes(&[0x3B, 0x33, 0x11, 0x00, 1, 0, 0, 0]), Some("item"), 0x11333B, "Fallout4.esm", "SeventySix.esm"),
            ("cont_cnto", "CONT", "CNTO", raw_bytes(&[0xB5, 0x73, 0x06, 0x00, 1, 0, 0, 0]), Some("item"), 0x0673B5, "Fallout4.esm", "SeventySix.esm"),
            ("npc_prkr", "NPC_", "PRKR", raw_bytes(&[0xF5, 0x64, 0x84, 0x00, 0x00]), Some("Perk"), 0x8464F5, "Output.esm", "Output.esm"),
        ] {
            let mut record = make_record(sig, &interner);
            push_field(&mut record, field, input);

            Fo76Fo4Hook
                .pre_translate(&mut make_ctx(&interner), &mut record)
                .unwrap();

            let reference = |record: &Record| -> FieldValue {
                let value = &record
                    .fields
                    .iter()
                    .find(|entry| entry.sig.as_str() == field)
                    .unwrap_or_else(|| panic!("{name}: {field} remains"))
                    .value;
                match subfield {
                    None => value.clone(),
                    Some(subfield) => {
                        let FieldValue::Struct(fields) = value else {
                            panic!("{name}: {field} should be structured");
                        };
                        named_value(fields, subfield, &interner)
                            .unwrap_or_else(|| panic!("{name}: {subfield}"))
                            .clone()
                    }
                }
            };
            let source_fk = FormKey {
                local,
                plugin: interner.intern("SeventySix.esm"),
            };
            let target_fk = FormKey {
                local,
                plugin: interner.intern(target_plugin),
            };
            assert_eq!(reference(&record), FieldValue::FormKey(source_fk), "{name}");

            let mut mapper = FormKeyMapper::new(
                Vec::new(),
                MapperOptions {
                    output_plugin_name: output_plugin.to_string(),
                    ..Default::default()
                },
                &interner,
            );
            mapper.add_mapping(source_fk, target_fk);
            mapper.rewrite_record(&mut record).unwrap();

            assert_eq!(reference(&record), FieldValue::FormKey(target_fk), "{name}");
        }
    }

    /// Translation carries `KWDA` across verbatim; nothing here special-cases a grip.
    /// The AlienRifle DOES end up without `AnimsGripRifleStraight`, but that happens
    /// later and generically, in `strip_generic_grips_from_owned_weapons`, which can
    /// see whether an additive third-person block was actually emitted for the weapon.
    /// Deciding it per-record here cannot: the block set does not exist yet.
    #[test]
    fn pre_translate_carries_weapon_keywords_across_untouched() {
        let interner = StringInterner::new();
        let mut record = make_record("WEAP", &interner);
        push_field(&mut record, "KSIZ", FieldValue::Uint(3));
        push_field(
            &mut record,
            "KWDA",
            FieldValue::List(vec![
                form_key_value(&interner, FO76_ANIMS_GRIP_RIFLE_STRAIGHT_FORM_ID),
                form_key_value(&interner, FO76_ANIMS_ALIEN_RIFLE_FORM_ID),
                form_key_value(&interner, 0x0F4AEA),
            ]),
        );

        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let keywords = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"KWDA")
            .expect("KWDA remains");
        let FieldValue::List(keywords) = &keywords.value else {
            panic!("KWDA should remain a decoded keyword list");
        };
        let keyword_locals: Vec<u32> = keywords
            .iter()
            .filter_map(|value| match value {
                FieldValue::FormKey(keyword) => Some(keyword.local),
                _ => None,
            })
            .collect();
        assert_eq!(
            keyword_locals,
            vec![
                FO76_ANIMS_GRIP_RIFLE_STRAIGHT_FORM_ID,
                FO76_ANIMS_ALIEN_RIFLE_FORM_ID,
                0x0F4AEA
            ]
        );
        assert_eq!(
            record
                .fields
                .iter()
                .find(|entry| entry.sig.0 == *b"KSIZ")
                .map(|entry| &entry.value),
            Some(&FieldValue::Uint(3))
        );
    }

    #[test]
    fn post_translate_normalizes_npc_raw_form_refs() {
        let mut interner = StringInterner::new();
        let mut record = make_record("NPC_", &mut interner);
        push_field(
            &mut record,
            "SNAM",
            raw_bytes(&[0x08, 0xC0, 0x3F, 0x00, 0xFE]),
        );
        push_field(
            &mut record,
            "CNTO",
            raw_bytes(&[0x84, 0xAB, 0x33, 0x00, 1, 0, 0, 0]),
        );
        push_field(&mut record, "INAM", raw_bytes(&[0x50, 0xE3, 0x04, 0x00]));

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let snam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "SNAM")
            .expect("SNAM remains");
        let FieldValue::Struct(snam_fields) = &snam.value else {
            panic!("SNAM should be structured");
        };
        let FieldValue::FormKey(faction) =
            named_value(snam_fields, "faction", &interner).expect("faction")
        else {
            panic!("faction should be a FormKey");
        };
        assert_eq!(faction.local, 0x3FC008);
        assert_eq!(interner.resolve(faction.plugin), Some("SeventySix.esm"));
        assert_eq!(
            named_value(snam_fields, "rank", &interner).expect("rank"),
            &raw_bytes(&[0xFE])
        );

        let cnto = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "CNTO")
            .expect("CNTO remains");
        let FieldValue::Struct(cnto_fields) = &cnto.value else {
            panic!("CNTO should be structured");
        };
        let FieldValue::FormKey(item) = named_value(cnto_fields, "item", &interner).expect("item")
        else {
            panic!("item should be a FormKey");
        };
        assert_eq!(item.local, 0x33AB84);
        assert_eq!(interner.resolve(item.plugin), Some("SeventySix.esm"));
        assert_eq!(
            named_value(cnto_fields, "count", &interner).expect("count"),
            &raw_bytes(&[1, 0, 0, 0])
        );

        let inam = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "INAM")
            .expect("INAM remains");
        let FieldValue::FormKey(death_item) = &inam.value else {
            panic!("INAM should be a FormKey");
        };
        assert_eq!(death_item.local, 0x04E350);
        assert_eq!(interner.resolve(death_item.plugin), Some("SeventySix.esm"));
    }

    #[test]
    fn post_translate_remaps_rd01_assassin_combat_style_to_ranged() {
        let mut interner = StringInterner::new();
        let fk = FormKey::parse("78BD9B@SeventySix.esm", &mut interner).unwrap();
        let mut record = Record::new(SigCode::from_str("NPC_").unwrap(), fk);
        let eid = interner.intern("RD01_Enc04_Assassin");
        let source_plugin = interner.intern("SeventySix.esm");
        record.eid = Some(eid);
        push_field(&mut record, "EDID", FieldValue::String(eid));
        push_field(
            &mut record,
            "ZNAM",
            FieldValue::FormKey(FormKey {
                local: CS_RAIDER_01_MELEE_FORM_ID,
                plugin: source_plugin,
            }),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let combat_style = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "ZNAM")
            .expect("combat style remains");
        let FieldValue::FormKey(fk) = &combat_style.value else {
            panic!("combat style should be a FormKey");
        };
        assert_eq!(fk.local, CS_RAIDER_RANGED_FORM_ID);
        assert_eq!(fk.plugin, source_plugin);
    }

    fn w05_nuke_reaction_vmad(base_outfit: Option<u32>) -> FieldValue {
        let masters = [FO4_MASTER_NAME.to_string()];
        let mut properties = Vec::new();
        if let Some(form_id) = base_outfit {
            properties.push(serde_json::json!({
                "propertyName": W05_ACTOR_NUKE_BASE_OUTFIT_PROPERTY,
                "Type": "Object",
                "Flags": VMAD_PROPERTY_FLAG_EDITED,
                "Value": {
                    "Alias": -1,
                    "FormID": {
                        "reference": {
                            "plugin": FO76_MASTER_NAME,
                            "object_id": format!("{form_id:06X}"),
                        },
                    },
                },
            }));
        }
        let payload = serde_json::json!({
            "Version": FO4_VMAD_VERSION,
            "Object Format": FO4_VMAD_OBJECT_FORMAT,
            "Scripts": [{
                "ScriptName": W05_ACTOR_NUKE_REACTION_SCRIPT,
                "Properties": properties,
            }],
        });
        raw_bytes(
            &build_vmad_bytes_from_payload(&payload, &masters, FO76_MASTER_NAME)
                .expect("W05 actor VMAD fixture must encode"),
        )
    }

    fn w05_nuke_reaction_base_outfit(record: &Record) -> u32 {
        let FieldValue::Bytes(bytes) = &record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"VMAD")
            .expect("W05 actor VMAD remains")
            .value
        else {
            panic!("W05 actor VMAD should remain bytes");
        };
        let masters = [FO4_MASTER_NAME.to_string()];
        let payload = esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json(
                bytes,
                &masters,
                FO76_MASTER_NAME,
                Some("NPC_"),
            )
            .expect("W05 actor VMAD must decode");
        let property = payload["Scripts"][0]["Properties"]
            .as_array()
            .expect("properties")
            .iter()
            .find(|property| {
                property["propertyName"].as_str() == Some(W05_ACTOR_NUKE_BASE_OUTFIT_PROPERTY)
            })
            .expect("BaseOutfit property");
        u32::from_str_radix(
            property["Value"]["FormID"]["reference"]["object_id"]
                .as_str()
                .expect("BaseOutfit object id"),
            16,
        )
        .expect("hex BaseOutfit object id")
    }

    fn w05_nuke_ctx<'a>(interner: &'a StringInterner, target_masters: &'a [String]) -> PairCtx<'a> {
        PairCtx::with_source_and_target(interner, FO76_MASTER_NAME, &[], target_masters)
    }

    #[test]
    fn post_translate_w05_nuke_base_outfit_backfills_from_doft_and_keeps_explicit() {
        let interner = StringInterner::new();
        let target_masters = [FO4_MASTER_NAME.to_string()];

        let mut record = make_record("NPC_", &interner);
        push_field(&mut record, "VMAD", w05_nuke_reaction_vmad(None));
        push_field(&mut record, "DOFT", form_key_value(&interner, 0x58E6C7));
        Fo76Fo4Hook
            .post_translate(&mut w05_nuke_ctx(&interner, &target_masters), &mut record)
            .unwrap();
        assert_eq!(w05_nuke_reaction_base_outfit(&record), 0x58E6C7);

        let mut record = make_record("NPC_", &interner);
        push_field(&mut record, "VMAD", w05_nuke_reaction_vmad(Some(0x01D984)));
        push_field(&mut record, "DOFT", form_key_value(&interner, 0x58E6C7));
        let mut ctx = w05_nuke_ctx(&interner, &target_masters);
        Fo76Fo4Hook.post_translate(&mut ctx, &mut record).unwrap();
        let once = record.fields.clone();
        Fo76Fo4Hook.post_translate(&mut ctx, &mut record).unwrap();
        assert_eq!(w05_nuke_reaction_base_outfit(&record), 0x01D984);
        assert_eq!(record.fields, once);
    }

    #[test]
    fn post_translate_w05_nuke_base_outfit_fails_closed() {
        let interner = StringInterner::new();
        let masters = [FO4_MASTER_NAME.to_string()];
        let duplicate_script_payload = serde_json::json!({
            "Version": FO4_VMAD_VERSION,
            "Object Format": FO4_VMAD_OBJECT_FORMAT,
            "Scripts": [
                {
                    "ScriptName": W05_ACTOR_NUKE_REACTION_SCRIPT,
                    "Properties": [],
                },
                {
                    "ScriptName": W05_ACTOR_NUKE_REACTION_SCRIPT,
                    "Properties": [],
                },
            ],
        });
        let duplicate_script_vmad = raw_bytes(
            &build_vmad_bytes_from_payload(&duplicate_script_payload, &masters, FO76_MASTER_NAME)
                .expect("duplicate-script VMAD fixture must encode"),
        );
        let doft = |local| ("DOFT", form_key_value(&interner, local));
        for (name, fields) in [
            (
                "duplicate_doft",
                vec![("VMAD", w05_nuke_reaction_vmad(None)), doft(0x58E6C7), doft(0x01D984)],
            ),
            (
                "duplicate_vmad",
                vec![
                    ("VMAD", w05_nuke_reaction_vmad(None)),
                    ("VMAD", w05_nuke_reaction_vmad(None)),
                    doft(0x58E6C7),
                ],
            ),
            (
                "duplicate_matching_script",
                vec![("VMAD", duplicate_script_vmad), doft(0x58E6C7)],
            ),
            (
                "malformed_vmad",
                vec![("VMAD", raw_bytes(b"W05_ActorNukeReactionScript")), doft(0x58E6C7)],
            ),
        ] {
            let mut record = make_record("NPC_", &interner);
            for (sig, value) in fields {
                push_field(&mut record, sig, value);
            }
            let original_fields = record.fields.clone();

            Fo76Fo4Hook
                .post_translate(&mut w05_nuke_ctx(&interner, &masters), &mut record)
                .unwrap();

            assert_eq!(record.fields, original_fields, "{name}");
        }
    }
