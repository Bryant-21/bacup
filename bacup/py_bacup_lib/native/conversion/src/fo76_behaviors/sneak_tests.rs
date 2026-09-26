use super::*;

fn object(class: &str, members: Vec<(&str, HkxValue)>) -> HkxObject {
    HkxObject {
        name: None,
        offset: 0,
        signature: 0,
        class_name: class.into(),
        members: members
            .into_iter()
            .map(|(name, value)| HkxMember {
                name: name.into(),
                value,
            })
            .collect(),
    }
}

fn s(value: &str) -> HkxValue {
    HkxValue::String {
        value: value.into(),
        is_null: false,
    }
}

fn transitions(targets: &[i32]) -> HkxObject {
    let items = targets
        .iter()
        .map(|&to| {
            HkxValue::Object(vec![
                HkxMember {
                    name: "eventId".into(),
                    value: HkxValue::I32(0),
                },
                HkxMember {
                    name: "toStateId".into(),
                    value: HkxValue::I32(to),
                },
            ])
        })
        .collect();
    object(
        "hkbStateMachineTransitionInfoArray",
        vec![("transitions", HkxValue::Array(items))],
    )
}

fn state(name: &str, id: i32, transitions: Option<usize>) -> HkxObject {
    object(
        "hkbStateMachineStateInfo",
        vec![
            ("name", s(name)),
            ("stateId", HkxValue::I32(id)),
            ("transitions", HkxValue::Pointer(transitions)),
        ],
    )
}

fn binding(path: &str, variable: i32) -> HkxValue {
    HkxValue::Object(vec![
        HkxMember {
            name: "memberPath".into(),
            value: s(path),
        },
        HkxMember {
            name: "variableIndex".into(),
            value: HkxValue::I32(variable),
        },
        HkxMember {
            name: "bitIndex".into(),
            value: HkxValue::I8(-1),
        },
        HkxMember {
            name: "bindingType".into(),
            value: HkxValue::I8(0),
        },
    ])
}

/// Mirrors FO76 3P `gunbehavior`'s `ReadyRelaxedSneakCover`.
fn fo76_gun_graph() -> HkxFile {
    let objects = vec![
        object(
            "hkbBehaviorGraphStringData",
            vec![(
                "variableNames",
                HkxValue::Array(vec![s("iIsInSneak"), s("other"), s("iSyncReadyRelaxSneak")]),
            )],
        ),
        object(
            "hkbStateMachine",
            vec![
                ("variableBindingSet", HkxValue::Pointer(None)),
                ("name", s("ReadyRelaxedSneakCover")),
                ("startStateId", HkxValue::I32(0)),
                ("syncVariableIndex", HkxValue::I32(2)),
                ("wrapAroundStateId", HkxValue::I32(0)),
                ("startStateMode", HkxValue::I8(1)),
                (
                    "states",
                    HkxValue::Array((2..6).map(|i| HkxValue::Pointer(Some(i))).collect()),
                ),
                ("wildcardTransitions", HkxValue::Pointer(Some(6))),
            ],
        ),
        state("ReadyState", 0, None),
        state("RelaxedState", 1, Some(7)),
        state("SneakState", 2, Some(8)),
        state("EnterCoverSneak", 3, Some(9)),
        transitions(&[2, 1, 0, 1, 1, 3]),
        transitions(&[0]),
        transitions(&[0]),
        transitions(&[2]),
        object(
            "hkbVariableBindingSet",
            vec![
                ("bindings", HkxValue::Array(vec![binding("enable", 1)])),
                ("indexOfBindingToEnable", HkxValue::I32(0)),
            ],
        ),
    ];
    HkxFile::from_tagxml(11, VERSION, objects)
}

fn targets(file: &HkxFile, index: usize) -> Vec<i32> {
    array(&file.objects()[index], "transitions")
        .iter()
        .map(|t| {
            let members = match t {
                HkxValue::Object(m) => m,
                _ => panic!("transition is not an object"),
            };
            let to = &members
                .iter()
                .find(|m| m.name == "toStateId")
                .unwrap()
                .value;
            number(to).unwrap() as i32
        })
        .collect()
}

#[test]
fn sync_start_is_rebound_to_engine_sneak_flag() {
    let mut file = fo76_gun_graph();
    assert_eq!(repair(&mut file).unwrap(), 1);

    let ids: Vec<_> = (2..6)
        .map(|i| {
            (
                string(&file.objects()[i], "name"),
                number(value(&file.objects()[i], "stateId").unwrap()).unwrap() as i32,
            )
        })
        .collect();
    assert_eq!(
        ids,
        [
            ("ReadyState".to_string(), 0),
            ("RelaxedState".to_string(), 2),
            ("SneakState".to_string(), 1),
            ("EnterCoverSneak".to_string(), 3),
        ]
    );
    assert_eq!(targets(&file, 6), [1, 2, 0, 2, 2, 3]);
    assert_eq!(targets(&file, 7), [0]);
    assert_eq!(targets(&file, 8), [0]);
    assert_eq!(targets(&file, 9), [1]);

    let machine = &file.objects()[1];
    assert_eq!(value(machine, "startStateMode"), Some(&HkxValue::I8(0)));
    assert_eq!(
        value(machine, "syncVariableIndex"),
        Some(&HkxValue::I32(-1))
    );
    let set_index = pointer(machine, "variableBindingSet").unwrap();
    let bindings = array(&file.objects()[set_index], "bindings");
    assert_eq!(bindings, vec![binding("startStateId", 0)]);
    assert_eq!(
        value(&file.objects()[set_index], "indexOfBindingToEnable"),
        Some(&HkxValue::I32(-1))
    );

    assert_eq!(repair(&mut file).unwrap(), 0);
}

#[test]
fn graphs_without_the_fo76_sync_variable_are_untouched() {
    let mut file = HkxFile::from_tagxml(
        11,
        VERSION,
        vec![object(
            "hkbBehaviorGraphStringData",
            vec![("variableNames", HkxValue::Array(vec![s("iIsInSneak")]))],
        )],
    );
    assert_eq!(repair(&mut file).unwrap(), 0);
    assert_eq!(file.objects().len(), 1);
}
