use havok_native::hkx::model::HkxFile;
use havok_native::hkx::types::HkxValue;

fn main() {
    let path = std::env::args().nth(1).expect("HKX path");
    let bytes = std::fs::read(path).expect("read HKX");
    let file = HkxFile::read(&bytes).expect("decode HKX");
    for (index, object) in file.objects().iter().enumerate() {
        if matches!(index, 144 | 145 | 147 | 148)
            || matches!(
            object.class_name.as_str(),
            "hkbBehaviorGraphData"
                | "hkbVariableValueSet"
                | "hkbModifierGenerator"
                | "hkbBlendingTransitionEffect"
                | "hkbVariableBindingSet"
        ) {
            println!(
                "BEGIN index={index} class={} signature=0x{:08x}",
                object.class_name, object.signature
            );
            for member in &object.members {
                match &member.value {
                    HkxValue::Array(values) => {
                        println!("  {}=Array(len={})", member.name, values.len());
                        if matches!(member.name.as_str(), "bindings" | "expressionsData") {
                            println!("    values={values:#?}");
                        }
                        if member.name == "wordVariableValues" {
                            for value_index in [0_usize, 1, 42, 78, values.len().saturating_sub(1)] {
                                if let Some(value) = values.get(value_index) {
                                    println!("    [{value_index}]={value:?}");
                                }
                            }
                        }
                    }
                    value => println!("  {}={value:?}", member.name),
                }
            }
            let data = file.packfile().section("__data__").expect("data section");
            let relative = object.offset - data.offset;
            let next = file
                .objects()
                .iter()
                .map(|candidate| candidate.offset - data.offset)
                .filter(|offset| *offset > relative)
                .min()
                .unwrap_or(data.data1 - data.offset);
            let global_fixups = file
                .packfile()
                .global_fixups
                .iter()
                .filter(|fixup| {
                    let source = fixup.source as usize;
                    source >= relative && source < next
                })
                .map(|fixup| {
                    let target = fixup.target as usize;
                    (
                        fixup.source as usize - relative,
                        file.objects()
                            .iter()
                            .position(|candidate| candidate.offset - data.offset == target),
                    )
                })
                .collect::<Vec<_>>();
            let local_fixups = file
                .packfile()
                .local_fixups
                .iter()
                .filter(|fixup| {
                    let source = fixup.source as usize;
                    source >= relative && source < next
                })
                .map(|fixup| {
                    (
                        fixup.source as usize - relative,
                        fixup.target as usize - relative,
                    )
                })
                .collect::<Vec<_>>();
            println!("  global_fixups={global_fixups:?}");
            println!("  local_fixups={local_fixups:?}");
            if object.class_name == "hkbBlendingTransitionEffect" {
                let bytes = file.source_bytes();
                if matches!(index, 66 | 69 | 75 | 81 | 87) {
                    println!("  object_len={}", next - relative);
                    for row in (0..next - relative).step_by(16) {
                        let end = (row + 16).min(next - relative);
                        println!(
                            "  raw[{row:03}..{end:03}]={:02x?}",
                            &bytes[object.offset + row..object.offset + end]
                        );
                    }
                }
                for subtract in [0_usize, 64, 72] {
                    let scalar = |descriptor_offset: usize, width: usize| {
                        let start = object.offset + descriptor_offset - subtract;
                        bytes.get(start..start + width).unwrap_or(&[])
                    };
                    println!(
                        "  candidate_subtract={subtract} self={:?} event={:?} duration={:?} fraction={:?} flags={:?} end={:?} curve={:?} align={:?}",
                        scalar(136, 1),
                        scalar(137, 1),
                        scalar(168, 4),
                        scalar(172, 4),
                        scalar(176, 2),
                        scalar(178, 1),
                        scalar(179, 1),
                        scalar(180, 2),
                    );
                }
            }
        }
    }
}
