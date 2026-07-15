//! `Transform` trait + `TransformRegistry`.

use rustc_hash::FxHashMap;

use super::super::record::FieldValue;
use super::super::sym::StringInterner;
use super::maps::YamlValue;

pub mod condition_functions;
pub mod dnam_to_intv;
pub mod enum_map;
pub mod fo76_misc_components;
pub mod fo76_scol_static;
pub mod remap_enum;
pub mod remap_formkey;
pub mod rewrite_creature_anam;
pub mod rgdl_to_bodt_default;
pub mod scale_nested;
pub mod strip_subfields;
pub mod translate_conditions;
pub mod translate_effects;
pub mod trim_languages;
pub mod venp_to_venv;
pub mod wrap_in_list;

/// Error variants from a transform application.
#[derive(Debug)]
pub enum TransformError {
    BadConfig(String),
    Unsupported(String),
    RuntimeError(String),
}

impl std::fmt::Display for TransformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadConfig(s) => write!(f, "bad transform config: {s}"),
            Self::Unsupported(s) => write!(f, "unsupported transform: {s}"),
            Self::RuntimeError(s) => write!(f, "transform runtime error: {s}"),
        }
    }
}

impl std::error::Error for TransformError {}

/// Minimal context passed into every `Transform::apply` call.
pub struct TransformCtx<'a> {
    pub interner: &'a StringInterner,
}

/// A named, stateless field-value transformation.
pub trait Transform: Send + Sync {
    fn name(&self) -> &'static str;
    fn apply(
        &self,
        ctx: &mut TransformCtx<'_>,
        value: &mut FieldValue,
        config: &YamlValue,
    ) -> Result<(), TransformError>;
}

/// Registry of named transforms, dispatched by string name.
#[derive(Default)]
pub struct TransformRegistry {
    by_name: FxHashMap<&'static str, Box<dyn Transform>>,
}

impl TransformRegistry {
    pub fn register(&mut self, t: Box<dyn Transform>) {
        self.by_name.insert(t.name(), t);
    }

    pub fn get(&self, name: &str) -> Option<&dyn Transform> {
        self.by_name.get(name).map(|b| &**b)
    }

    pub fn names(&self) -> impl Iterator<Item = &&'static str> {
        self.by_name.keys()
    }
}

/// Build a registry pre-populated with every registered transform.
pub fn default_registry() -> TransformRegistry {
    let mut r = TransformRegistry::default();
    r.register(Box::new(IdentityTransform));
    r.register(Box::new(dnam_to_intv::DnamToIntvTransform));
    r.register(Box::new(enum_map::EnumMapTransform));
    r.register(Box::new(fo76_misc_components::Fo76MiscComponentsTransform));
    r.register(Box::new(fo76_scol_static::Fo76ScolStaticTransform));
    r.register(Box::new(remap_enum::RemapEnumTransform));
    r.register(Box::new(remap_formkey::RemapFormkeyTransform));
    r.register(Box::new(
        rewrite_creature_anam::RewriteCreatureAnamTransform,
    ));
    r.register(Box::new(rgdl_to_bodt_default::RgdlToBodtDefaultTransform));
    r.register(Box::new(scale_nested::ScaleNestedTransform));
    r.register(Box::new(strip_subfields::StripSubfieldsTransform));
    r.register(Box::new(translate_conditions::TranslateConditionsTransform));
    r.register(Box::new(translate_effects::TranslateEffectsTransform));
    r.register(Box::new(trim_languages::TrimLanguagesTransform));
    r.register(Box::new(venp_to_venv::VenpToVenvTransform));
    r.register(Box::new(wrap_in_list::WrapInListTransform));
    r
}

/// No-op transform — used in tests and as a placeholder.
pub struct IdentityTransform;

impl Transform for IdentityTransform {
    fn name(&self) -> &'static str {
        "identity"
    }

    fn apply(
        &self,
        _ctx: &mut TransformCtx<'_>,
        _value: &mut FieldValue,
        _config: &YamlValue,
    ) -> Result<(), TransformError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_registry_dispatches_by_name() {
        let mut registry = TransformRegistry::default();
        registry.register(Box::new(IdentityTransform));
        let transform = registry.get("identity").expect("identity registered");
        assert_eq!(transform.name(), "identity");
    }

    #[test]
    fn identity_transform_is_a_no_op() {
        let mut registry = TransformRegistry::default();
        registry.register(Box::new(IdentityTransform));
        let transform = registry.get("identity").unwrap();
        let mut interner = StringInterner::new();
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        let mut value = FieldValue::Int(42);
        transform
            .apply(&mut ctx, &mut value, &serde_json::Value::Null)
            .unwrap();
        assert_eq!(value, FieldValue::Int(42));
    }
}
