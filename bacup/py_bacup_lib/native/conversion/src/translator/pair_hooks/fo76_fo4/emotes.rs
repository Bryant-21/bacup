use super::*;
use crate::sym::StringInterner;

const LOOP_CULL_WEAPONS_EVENT: &str = "dyn_ActivationLoopCullWeapons";
const CULL_WEAPONS_EVENT: &str = "dyn_ActivationCullWeapons";
// No graph at these FO4 paths routes the FO76 looping cull event (the shipped
// augmented copies list it but have no consuming transition); only the
// FO76-native Actors\B21_FO76\PowerArmor graphs do, so those keep it.
const GRAPHS_WITHOUT_LOOP_CULL_ROUTE: [&str; 2] = [
    "actors\\character\\behaviors\\raiderrootbehavior.hkx",
    "actors\\powerarmor\\behaviors\\powerarmorbehavior.hkx",
];

impl Fo76Fo4Hook {
    pub(super) fn adapt_emote_idle(interner: &StringInterner, record: &mut Record) {
        if record.sig.0 != *b"IDLE"
            || !interner
                .resolve(record.form_key.plugin)
                .is_some_and(|name| name.eq_ignore_ascii_case(FO76_MASTER_NAME))
        {
            return;
        }
        let stem = match (
            record.form_key.local,
            record.eid.and_then(|eid| interner.resolve(eid)),
        ) {
            (0x3AC5F1, Some("Emote_Wave1Anim")) => "Wave",
            (0x4786D8, Some("Emote_Salute1Anim")) => "Salute",
            _ => return Self::remap_unrouted_loop_cull_event(interner, record),
        };
        let expected_file = format!("$(Subgraph)\\{stem}.hkx");
        let mut source_event = false;
        let mut source_file = false;
        let mut source_graph = false;
        for field in &record.fields {
            if let FieldValue::String(value) = field.value {
                let text = interner.resolve(value).unwrap_or_default();
                match &field.sig.0 {
                    b"ENAM" => source_event = text.eq_ignore_ascii_case(LOOP_CULL_WEAPONS_EVENT),
                    b"GNAM" => source_file = text.eq_ignore_ascii_case(&expected_file),
                    b"DNAM" => {
                        source_graph = text.eq_ignore_ascii_case(
                            "actors\\Character\\Behaviors\\RaiderRootBehavior.hkx",
                        )
                    }
                    _ => {}
                }
            }
        }
        if !(source_event && source_file && source_graph) {
            record
                .warnings
                .push(interner.intern("emote IDLE source route changed; no FO4 playback rewrite"));
            return;
        }
        for field in &mut record.fields {
            match &field.sig.0 {
                b"ENAM" => field.value = FieldValue::String(interner.intern(CULL_WEAPONS_EVENT)),
                b"GNAM" => {
                    field.value = FieldValue::String(interner.intern(&format!(
                        "Actors\\Character\\Animations\\B21\\Emotes\\{stem}.hkx"
                    )))
                }
                _ => {}
            }
        }
    }

    fn remap_unrouted_loop_cull_event(interner: &StringInterner, record: &mut Record) {
        let field_is = |field: &FieldEntry, sig: &[u8; 4], accept: &dyn Fn(&str) -> bool| {
            field.sig.0 == *sig
                && matches!(field.value, FieldValue::String(value)
                if interner.resolve(value).is_some_and(|text| accept(text)))
        };
        let targets_unrouted_graph = record.fields.iter().any(|field| {
            field_is(field, b"DNAM", &|text| {
                let path = text.replace('/', "\\");
                GRAPHS_WITHOUT_LOOP_CULL_ROUTE
                    .iter()
                    .any(|graph| path.eq_ignore_ascii_case(graph))
            })
        });
        if !targets_unrouted_graph {
            return;
        }
        for field in &mut record.fields {
            if field_is(field, b"ENAM", &|text| {
                text.eq_ignore_ascii_case(LOOP_CULL_WEAPONS_EVENT)
            }) {
                field.value = FieldValue::String(interner.intern(CULL_WEAPONS_EVENT));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_matching_emote_idles_and_unrouted_graphs_receive_fo4_routes() {
        let interner = StringInterner::new();
        let plugin = interner.intern(FO76_MASTER_NAME);
        let make = |id, eid: &str, event: &str, stem: &str| {
            let mut record = Record::new(SigCode(*b"IDLE"), FormKey { plugin, local: id });
            record.eid = Some(interner.intern(eid));
            for (sig, value) in [
                (
                    *b"DNAM",
                    "actors\\Character\\Behaviors\\RaiderRootBehavior.hkx".to_string(),
                ),
                (*b"ENAM", event.to_string()),
                (*b"GNAM", format!("$(Subgraph)\\{stem}.hkx")),
            ] {
                record.fields.push(FieldEntry {
                    sig: SubrecordSig(sig),
                    value: FieldValue::String(interner.intern(&value)),
                });
            }
            record
        };
        let mut wave = make(
            0x3AC5F1,
            "Emote_Wave1Anim",
            "dyn_ActivationLoopCullWeapons",
            "Wave",
        );
        Fo76Fo4Hook::adapt_emote_idle(&interner, &mut wave);
        assert!(wave.fields.iter().any(|field| field.sig.0 == *b"ENAM" &&
            matches!(field.value, FieldValue::String(value) if interner.resolve(value) == Some("dyn_ActivationCullWeapons"))));
        assert!(wave.fields.iter().any(|field| field.sig.0 == *b"GNAM" &&
            matches!(field.value, FieldValue::String(value) if interner.resolve(value) == Some("Actors\\Character\\Animations\\B21\\Emotes\\Wave.hkx"))));
        let mut salute = make(
            0x4786D8,
            "Emote_Salute1Anim",
            "dyn_ActivationLoopCullWeapons",
            "Salute",
        );
        Fo76Fo4Hook::adapt_emote_idle(&interner, &mut salute);
        assert!(salute.fields.iter().any(|field| field.sig.0 == *b"GNAM" &&
            matches!(field.value, FieldValue::String(value) if interner.resolve(value) == Some("Actors\\Character\\Animations\\B21\\Emotes\\Salute.hkx"))));
        let mut changed = make(0x3AC5F1, "Emote_Wave1Anim", "unexpected", "Wave");
        Fo76Fo4Hook::adapt_emote_idle(&interner, &mut changed);
        assert_eq!(
            changed.fields[1].value,
            FieldValue::String(interner.intern("unexpected"))
        );
        let mut unrelated = make(
            0x3AC5F2,
            "OtherIdle",
            "dyn_ActivationLoopCullWeapons",
            "Wave",
        );
        Fo76Fo4Hook::adapt_emote_idle(&interner, &mut unrelated);
        assert_eq!(
            unrelated.fields[1].value,
            FieldValue::String(interner.intern("dyn_ActivationCullWeapons"))
        );
        assert_eq!(
            unrelated.fields[2].value,
            FieldValue::String(interner.intern("$(Subgraph)\\Wave.hkx"))
        );

        let make = |id, graph: &str, event: &str| {
            let mut record = Record::new(SigCode(*b"IDLE"), FormKey { plugin, local: id });
            for (sig, value) in [(*b"DNAM", graph), (*b"ENAM", event)] {
                record.fields.push(FieldEntry {
                    sig: SubrecordSig(sig),
                    value: FieldValue::String(interner.intern(value)),
                });
            }
            record
        };
        let event = |record: &Record| match record.fields[1].value {
            FieldValue::String(value) => interner.resolve(value).unwrap_or_default().to_string(),
            _ => String::new(),
        };
        for (id, graph) in [
            (
                0x45F108,
                "Actors\\Character\\Behaviors\\RaiderRootBehavior.hkx",
            ),
            (
                0x451099,
                "Actors\\PowerArmor\\Behaviors\\PowerArmorBehavior.hkx",
            ),
            (
                0x45F109,
                "actors/character/behaviors/raiderrootbehavior.hkx",
            ),
        ] {
            let mut idle = make(id, graph, "dyn_ActivationLoopCullWeapons");
            Fo76Fo4Hook::adapt_emote_idle(&interner, &mut idle);
            assert_eq!(event(&idle), "dyn_ActivationCullWeapons", "{graph}");
        }
        let mut fo76_graph = make(
            0x1,
            "Actors\\B21_FO76\\PowerArmor\\Behaviors\\PowerArmorBehavior.hkx",
            "dyn_ActivationLoopCullWeapons",
        );
        Fo76Fo4Hook::adapt_emote_idle(&interner, &mut fo76_graph);
        assert_eq!(event(&fo76_graph), "dyn_ActivationLoopCullWeapons");
        let mut other_event = make(
            0x43BAEF,
            "Actors\\Character\\Behaviors\\RaiderRootBehavior.hkx",
            "dyn_Activation",
        );
        Fo76Fo4Hook::adapt_emote_idle(&interner, &mut other_event);
        assert_eq!(event(&other_event), "dyn_Activation");
        let mut fo4_owned = make(
            0x2,
            "Actors\\Character\\Behaviors\\RaiderRootBehavior.hkx",
            "dyn_ActivationLoopCullWeapons",
        );
        fo4_owned.form_key.plugin = interner.intern("Fallout4.esm");
        Fo76Fo4Hook::adapt_emote_idle(&interner, &mut fo4_owned);
        assert_eq!(event(&fo4_owned), "dyn_ActivationLoopCullWeapons");
    }
}
