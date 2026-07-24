    fn make_ctx(interner: &StringInterner) -> PairCtx<'_> {
        PairCtx { interner }
    }

    fn make_record(sig: &str, interner: &StringInterner) -> Record {
        let fk = FormKey::parse("000800@SeventySix.esm", interner).unwrap();
        Record::new(SigCode::from_str(sig).unwrap(), fk)
    }

    fn push_field(record: &mut Record, sig: &str, value: FieldValue) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        });
    }

    fn form_key_value(interner: &StringInterner, local: u32) -> FieldValue {
        FieldValue::FormKey(FormKey {
            local,
            plugin: interner.intern(FO76_MASTER_NAME),
        })
    }

    struct QustVmadFixture {
        bytes: Vec<u8>,
        fragment_version_offset: usize,
        alias_version_offsets: Vec<usize>,
        alias_object_format_offsets: Vec<usize>,
        alias_property_type_offsets: Vec<usize>,
    }

    fn qust_vmad_fixture(aliases: &[(i16, &[&str])]) -> QustVmadFixture {
        fn write_string(bytes: &mut Vec<u8>, value: &str) {
            bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&FO76_VMAD_VERSION.to_le_bytes());
        bytes.extend_from_slice(&FO76_VMAD_OBJECT_FORMAT.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        let fragment_version_offset = bytes.len();
        bytes.push(FO76_QUST_FRAGMENT_VERSION);
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        write_string(&mut bytes, "Fragments:Quests:QF_TestQuest_00000800");
        bytes.push(0);
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        write_string(&mut bytes, "Alias_Player");
        bytes.push(1);
        bytes.push(1);
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&(-1_i16).to_le_bytes());
        bytes.extend_from_slice(&0x003E_EA16_u32.to_le_bytes());
        bytes.extend_from_slice(&(aliases.len() as u16).to_le_bytes());
        let mut alias_version_offsets = Vec::new();
        let mut alias_object_format_offsets = Vec::new();
        let mut alias_property_type_offsets = Vec::new();
        for (alias_id, scripts) in aliases {
            bytes.extend_from_slice(&0_u16.to_le_bytes());
            bytes.extend_from_slice(&alias_id.to_le_bytes());
            bytes.extend_from_slice(&0x003E_EA16_u32.to_le_bytes());
            alias_version_offsets.push(bytes.len());
            bytes.extend_from_slice(&FO76_VMAD_ALIAS_VERSION.to_le_bytes());
            alias_object_format_offsets.push(bytes.len());
            bytes.extend_from_slice(&FO76_VMAD_OBJECT_FORMAT.to_le_bytes());
            bytes.extend_from_slice(&(scripts.len() as u16).to_le_bytes());
            for script in *scripts {
                write_string(&mut bytes, script);
                bytes.push(0);
                bytes.extend_from_slice(&4_u16.to_le_bytes());

                write_string(&mut bytes, "RemoveItemsOnShutDown");
                alias_property_type_offsets.push(bytes.len());
                bytes.push(5);
                bytes.push(1);
                bytes.push(1);

                write_string(&mut bytes, "StageToSet");
                bytes.push(3);
                bytes.push(1);
                bytes.extend_from_slice(&300_i32.to_le_bytes());

                write_string(&mut bytes, "ItemCountTextVar");
                bytes.push(2);
                bytes.push(1);
                write_string(&mut bytes, "PageCount");

                write_string(&mut bytes, "RequiredItems");
                bytes.push(11);
                bytes.push(1);
                bytes.extend_from_slice(&2_i32.to_le_bytes());
                for form_id in [0x003E_EA3C_u32, 0x003E_EA3D] {
                    bytes.extend_from_slice(&0_u16.to_le_bytes());
                    bytes.extend_from_slice(&(-1_i16).to_le_bytes());
                    bytes.extend_from_slice(&form_id.to_le_bytes());
                }
            }
        }
        QustVmadFixture {
            bytes,
            fragment_version_offset,
            alias_version_offsets,
            alias_object_format_offsets,
            alias_property_type_offsets,
        }
    }

    fn qust_vmad_with_alias_scripts(aliases: &[(i16, &[&str])]) -> FieldValue {
        FieldValue::Bytes(SmallVec::from_vec(qust_vmad_fixture(aliases).bytes))
    }

    fn qust_vmad_with_remove_players_aliases(alias_ids: &[i16]) -> FieldValue {
        fn write_string(bytes: &mut Vec<u8>, value: &str) {
            bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&FO76_VMAD_VERSION.to_le_bytes());
        bytes.extend_from_slice(&FO76_VMAD_OBJECT_FORMAT.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        write_string(&mut bytes, "DefaultQuestRemovePlayersScript");
        bytes.push(0);
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        write_string(&mut bytes, "playerAliases");
        bytes.push(11);
        bytes.push(1);
        bytes.extend_from_slice(&(alias_ids.len() as i32).to_le_bytes());
        for alias_id in alias_ids {
            bytes.extend_from_slice(&0_u16.to_le_bytes());
            bytes.extend_from_slice(&alias_id.to_le_bytes());
            bytes.extend_from_slice(&0x0053_AF40_u32.to_le_bytes());
        }
        bytes.push(FO76_QUST_FRAGMENT_VERSION);
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        write_string(&mut bytes, "");
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    fn qust_vmad_with_fragment_alias_property(
        property_name: &str,
        alias_id: i16,
    ) -> FieldValue {
        fn write_string(bytes: &mut Vec<u8>, value: &str) {
            bytes.extend_from_slice(&(value.len() as u16).to_le_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&FO76_VMAD_VERSION.to_le_bytes());
        bytes.extend_from_slice(&FO76_VMAD_OBJECT_FORMAT.to_le_bytes());
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.push(FO76_QUST_FRAGMENT_VERSION);
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        write_string(&mut bytes, "Fragments:Quests:QF_TestQuest_00000800");
        bytes.push(0);
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        write_string(&mut bytes, property_name);
        bytes.push(1);
        bytes.push(1);
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        bytes.extend_from_slice(&alias_id.to_le_bytes());
        bytes.extend_from_slice(&0x0000_0800_u32.to_le_bytes());
        bytes.extend_from_slice(&[0; 9]);
        write_string(&mut bytes, "Fragments:Quests:QF_TestQuest_00000800");
        write_string(&mut bytes, "Fragment_Stage_0100_Item_00");
        bytes.extend_from_slice(&0_u16.to_le_bytes());
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    fn push_qust_event_alias(
        record: &mut Record,
        alias_id: u32,
        flags: u32,
        event: u32,
        event_data: u32,
    ) {
        push_field(
            record,
            "ALST",
            FieldValue::Bytes(SmallVec::from_vec(alias_id.to_le_bytes().to_vec())),
        );
        push_field(record, "ALID", FieldValue::Bytes(SmallVec::new()));
        push_field(
            record,
            "FNAM",
            FieldValue::Bytes(SmallVec::from_vec(flags.to_le_bytes().to_vec())),
        );
        push_field(
            record,
            "ALFE",
            FieldValue::Bytes(SmallVec::from_vec(event.to_le_bytes().to_vec())),
        );
        push_field(
            record,
            "ALFD",
            FieldValue::Bytes(SmallVec::from_vec(event_data.to_le_bytes().to_vec())),
        );
        push_field(record, "ALED", FieldValue::None);
    }

    fn qust_alias_flags(record: &Record) -> Vec<u32> {
        record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"FNAM")
            .map(|entry| field_value_to_u32(&entry.value).expect("u32 alias flags"))
            .collect()
    }

    fn translate_daim_event_alias_with_vmad(
        interner: &StringInterner,
        vmad_bytes: Vec<u8>,
    ) -> Record {
        let mut record = make_record("QUST", interner);
        push_field(
            &mut record,
            "VMAD",
            FieldValue::Bytes(SmallVec::from_vec(vmad_bytes)),
        );
        push_field(
            &mut record,
            "ANAM",
            FieldValue::Bytes(SmallVec::from_vec(2_u32.to_le_bytes().to_vec())),
        );
        push_qust_event_alias(
            &mut record,
            1,
            0,
            FO76_QUEST_EVENT_SCPT,
            FO76_QUEST_EVENT_REFERENCE3,
        );
        Fo76Fo4Hook
            .pre_translate(&mut make_ctx(interner), &mut record)
            .unwrap();
        record
    }

    fn assert_qust_event_alias_used_generic_fallback(record: &Record) {
        assert!(
            !record
                .fields
                .iter()
                .any(|entry| { matches!(&entry.sig.0, b"ALFE" | b"ALFD" | b"ALFR") })
        );
        assert_eq!(qust_alias_flags(record), vec![QUST_ALIAS_OPTIONAL_FLAG]);
    }

    #[test]
    fn pre_translate_upper_body_skin_drops_fo76_attachment_slots() {
        let interner = StringInterner::new();
        let mut record = make_record("ARMA", &interner);
        let attachment_slots = (41..=45).fold(0_u64, |mask, slot| mask | (1 << (slot - 30)));
        push_field(
            &mut record,
            "BOD2",
            FieldValue::Uint((1 << (33 - 30)) | attachment_slots | (1 << (60 - 30))),
        );
        push_field(
            &mut record,
            "XFLG",
            FieldValue::Uint(FO76_ARMA_XFLG_HAS_UPPER_BODY_SKIN),
        );

        let hook = Fo76Fo4Hook;
        hook.pre_translate(&mut make_ctx(&interner), &mut record)
            .unwrap();

        let mask = record
            .fields
            .iter()
            .find(|entry| entry.sig.0 == *b"BOD2")
            .and_then(|entry| match entry.value {
                FieldValue::Uint(mask) => Some(mask),
                _ => None,
            })
            .expect("ARMA BOD2 mask");
        assert_eq!(mask, (1 << (33 - 30)) | (1 << (60 - 30)));
    }
