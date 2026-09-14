fn runtime_magic_plan(record: &Record, interner: &StringInterner) -> MagicLoweringPlan {
    classify_magic_component(record, interner)
        .plan()
        .cloned()
        .expect("fixture must have a generic lowering plan")
}

fn runtime_magic_lower(record: &mut Record, interner: &StringInterner) -> MagicLoweringReceipt {
    let plan = runtime_magic_plan(record, interner);
    lower_supported_magic_record(record, &plan, interner).unwrap()
}

#[test]
fn classifies_jzargo_fixture_by_magic_shape_without_identity() {
    let interner = StringInterner::new();
    let fixtures = [
        (
            jzargo_scroll_source(&interner),
            MagicComponent::ScrollConsumable,
            true,
        ),
        (
            jzargo_outer_spell_source(&interner),
            MagicComponent::Spell,
            true,
        ),
        (
            jzargo_inner_spell_source(&interner),
            MagicComponent::Spell,
            true,
        ),
        (
            jzargo_outer_effect_source(&interner),
            MagicComponent::CloakEffect,
            true,
        ),
        (
            jzargo_inner_effect_source(&interner),
            MagicComponent::AimedFireDamageEffect,
            false,
        ),
    ];

    for (mut fixture, component, needs_link) in fixtures {
        fixture.form_key.local ^= 0x0055AA;
        fixture.eid = Some(interner.intern("IdentityDoesNotSelectMagicSupport"));
        let support = classify_magic_component(&fixture, &interner);
        assert_eq!(support.plan().unwrap().component, component);
        assert_eq!(
            matches!(support, MagicSupport::RequiresLinkedRecords(_)),
            needs_link
        );
    }
}

#[test]
fn reports_semantic_link_requirements_from_record_edges() {
    let interner = StringInterner::new();
    for (record, role, local, signature) in [
        (
            jzargo_scroll_source(&interner),
            MagicLinkedRole::BaseEffect,
            0x097EE2,
            "MGEF",
        ),
        (
            jzargo_outer_spell_source(&interner),
            MagicLinkedRole::BaseEffect,
            0x097EE2,
            "MGEF",
        ),
        (
            jzargo_inner_spell_source(&interner),
            MagicLinkedRole::BaseEffect,
            0x097EE1,
            "MGEF",
        ),
        (
            jzargo_outer_effect_source(&interner),
            MagicLinkedRole::AssociatedSpell,
            0x097EE0,
            "SPEL",
        ),
    ] {
        let MagicSupport::RequiresLinkedRecords(requirements) =
            classify_magic_component(&record, &interner)
        else {
            panic!("fixture must expose a linked-record requirement")
        };
        assert_eq!(requirements.linked_records.len(), 1);
        assert_eq!(requirements.linked_records[0].role, role);
        assert_eq!(requirements.linked_records[0].form_key.local, local);
        assert_eq!(requirements.linked_records[0].signature.as_str(), signature);
    }
}

#[test]
fn generic_lowering_preserves_cast_delivery_effect_rows_and_vmad() {
    let interner = StringInterner::new();
    let mut scroll = jzargo_scroll_source(&interner);
    let mut outer_spell = jzargo_outer_spell_source(&interner);
    let mut inner_spell = jzargo_inner_spell_source(&interner);
    let mut cloak = jzargo_outer_effect_source(&interner);
    let mut damage = jzargo_inner_effect_source(&interner);

    for record in [&mut scroll, &mut outer_spell, &mut inner_spell, &mut cloak] {
        runtime_magic_lower(record, &interner);
    }
    let receipt = runtime_magic_lower(&mut damage, &interner);

    assert_eq!(scroll.sig.as_str(), "ALCH");
    assert_eq!(
        jzargo_named(&scroll, *b"EFIT", "Duration", &interner),
        &FieldValue::Uint(30)
    );
    assert_eq!(
        jzargo_named(&outer_spell, *b"SPIT", "TargetType", &interner),
        &FieldValue::Uint(0)
    );
    assert_eq!(
        jzargo_named(&inner_spell, *b"SPIT", "CastType", &interner),
        &FieldValue::Uint(2)
    );
    assert_eq!(
        jzargo_named(&inner_spell, *b"SPIT", "TargetType", &interner),
        &FieldValue::Uint(2)
    );
    assert_eq!(
        jzargo_named(&inner_spell, *b"EFIT", "Duration", &interner),
        &FieldValue::Uint(5)
    );
    jzargo_assert_form(
        jzargo_named(&cloak, *b"DATA", "AssocItem", &interner),
        0x097EE0,
        "Skyrim.esm",
        &interner,
    );
    jzargo_assert_form(
        jzargo_named(&damage, *b"DATA", "Projectile", &interner),
        0x204172,
        "Fallout4.esm",
        &interner,
    );
    assert_eq!(receipt.preserved_vmad_fields, 1);
    assert_eq!(
        receipt.donor_roles,
        vec![
            MagicDonorRole::HealthActorValue,
            MagicDonorRole::EnergyResistance,
            MagicDonorRole::FireHitShader,
            MagicDonorRole::AimedFlameProjectile,
        ]
    );
    assert!(damage.fields.iter().any(|field| field.sig.0 == *b"VMAD"));
}

#[test]
fn rejects_shapes_outside_the_bounded_magic_capability() {
    let interner = StringInterner::new();

    let mut touch_spell = jzargo_inner_spell_source(&interner);
    let FieldValue::Struct(spell_data) = &mut touch_spell
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"SPIT")
        .unwrap()
        .value
    else {
        panic!("fixture SPIT must be structured")
    };
    spell_data
        .iter_mut()
        .find(|(name, _)| interner.resolve(*name) == Some("TargetType"))
        .unwrap()
        .1 = FieldValue::String(interner.intern("Touch"));
    assert_eq!(
        classify_magic_component(&touch_spell, &interner),
        MagicSupport::Unsupported {
            reason: MagicUnsupportedReason::CastDelivery,
        }
    );

    let mut frost_effect = jzargo_inner_effect_source(&interner);
    let FieldValue::Struct(effect_data) = &mut frost_effect
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"DATA")
        .unwrap()
        .value
    else {
        panic!("fixture DATA must be structured")
    };
    effect_data
        .iter_mut()
        .find(|(name, _)| interner.resolve(*name) == Some("ResistValue"))
        .unwrap()
        .1 = FieldValue::String(interner.intern("ResistFrost"));
    assert_eq!(
        classify_magic_component(&frost_effect, &interner),
        MagicSupport::Unsupported {
            reason: MagicUnsupportedReason::Resistance,
        }
    );

    let mut multi_effect = jzargo_inner_spell_source(&interner);
    jzargo_push(
        &mut multi_effect,
        *b"EFID",
        FieldValue::FormKey(FormKey {
            local: 0x012345,
            plugin: interner.intern("Skyrim.esm"),
        }),
    );
    assert_eq!(
        classify_magic_component(&multi_effect, &interner),
        MagicSupport::Unsupported {
            reason: MagicUnsupportedReason::EffectCardinality,
        }
    );
}

#[test]
fn validates_generic_magic_donor_catalog_and_rejects_plan_drift() {
    let interner = StringInterner::new();
    for role in [
        MagicDonorRole::HealthActorValue,
        MagicDonorRole::EnergyResistance,
        MagicDonorRole::FireHitShader,
        MagicDonorRole::AimedFlameProjectile,
    ] {
        let identity = magic_donor_identity(role, &interner);
        let mut donor = Record::new(identity.signature, identity.form_key);
        donor.eid = Some(interner.intern(identity.editor_id));
        jzargo_push(
            &mut donor,
            *b"EDID",
            FieldValue::String(interner.intern(identity.editor_id)),
        );
        assert_eq!(
            validate_magic_donor(&donor, role, &interner).unwrap(),
            identity
        );
    }

    let mut record = jzargo_inner_spell_source(&interner);
    let plan = runtime_magic_plan(&record, &interner);
    let FieldValue::Struct(data) = &mut record
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"SPIT")
        .unwrap()
        .value
    else {
        panic!("fixture SPIT must be structured")
    };
    data.iter_mut()
        .find(|(name, _)| interner.resolve(*name) == Some("TargetType"))
        .unwrap()
        .1 = FieldValue::String(interner.intern("Touch"));
    assert!(matches!(
        lower_supported_magic_record(&mut record, &plan, &interner),
        Err(MagicContractError::PlanDrift { .. })
    ));
}
