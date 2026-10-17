use std::collections::BTreeSet;

use super::manifest::{CreatureManifest, ValidationErrors};

pub(super) fn validate(manifest: &CreatureManifest, errors: &mut ValidationErrors) {
    let mut keys = BTreeSet::new();
    let mut sequences = BTreeSet::new();
    for binding in &manifest.property_sequences {
        if !manifest
            .clips
            .iter()
            .any(|clip| clip.name == binding.clip_name)
            || !keys.insert((
                &binding.clip_name,
                binding.crop_start_bits,
                binding.crop_end_bits,
                binding.looping,
            ))
            || [binding.crop_start_bits, binding.crop_end_bits]
                .iter()
                .any(|bits| {
                    let value = f32::from_bits(*bits);
                    !value.is_finite() || value < 0.0
                })
            || binding.source_float_slots.is_empty()
        {
            errors.push(
                "invalid_property_sequence",
                format!("invalid NIF sequence binding for {}", binding.clip_name),
            );
        }
        for name in &binding.sequence_names {
            if !name.starts_with("B21CP_") || !sequences.insert(name) {
                errors.push(
                    "invalid_property_sequence",
                    format!("nonunique creature NIF sequence {name:?}"),
                );
            }
        }
    }
}
