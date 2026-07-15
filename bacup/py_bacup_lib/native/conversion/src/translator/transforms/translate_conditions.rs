//! `translate_conditions` transform — clean CTDA condition lists for the target game.
//!
//! Port of `RecordTranslator._translate_conditions` (Python line 1655).
//!
//! For each condition Struct in the list:
//! 1. If `Data.Function` is a numeric `Int`/`Uint`/`Float` value, look it up in
//!    the source-game condition function table.  Unknown codes → drop the condition.
//! 2. If `ComparisonValue` is present but `CompareOperator` is absent, default
//!    `CompareOperator` to `"EqualTo"`.
//! 3. Remap all String FormKeys via source_esm → target_esm.
//!
//! NOTE: `normalize_legacy_condition` (FO4 target hook) is NOT called here —
//! that is a target-hook concern wired separately.  A TODO is left below.
//!
//! Config keys:
//! - `source_esm` : e.g. `"SeventySix.esm"`
//! - `target_esm` : e.g. `"Fallout4.esm"`
//! - `source_game`: optional; defaults to `"fo76"` for the condition function table

use super::super::super::record::FieldValue;
use super::super::super::sym::{StringInterner, Sym};
use super::super::maps::YamlValue;
use super::condition_functions::condition_functions_for;
use super::scale_nested::deep_remap_formkey;
use super::{Transform, TransformCtx, TransformError};
use rustc_hash::FxHashSet;
use std::sync::{Mutex, OnceLock};

pub struct TranslateConditionsTransform;

impl Transform for TranslateConditionsTransform {
    fn name(&self) -> &'static str {
        "translate_conditions"
    }

    fn apply(
        &self,
        ctx: &mut TransformCtx<'_>,
        value: &mut FieldValue,
        config: &YamlValue,
    ) -> Result<(), TransformError> {
        let source_esm = config
            .get("source_esm")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let target_esm = config
            .get("target_esm")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let source_game = config
            .get("source_game")
            .and_then(|v| v.as_str())
            .unwrap_or("fo76");

        let list = match value {
            FieldValue::List(l) => l,
            _ => return Ok(()),
        };

        let do_remap = !source_esm.is_empty() && !target_esm.is_empty();
        let fn_table = condition_functions_for(source_game);

        let data_key = ctx.interner.intern("Data");
        let function_key = ctx.interner.intern("Function");
        let comparison_value_key = ctx.interner.intern("ComparisonValue");
        let compare_operator_key = ctx.interner.intern("CompareOperator");
        let equal_to_sym = ctx.interner.intern("EqualTo");

        let mut cleaned: Vec<FieldValue> = Vec::with_capacity(list.len());

        for cond in list.drain(..) {
            // TODO: call normalize_legacy_condition target hook here.
            match cond {
                FieldValue::Struct(mut pairs) => {
                    // --- Step 1: remap numeric Data.Function ---
                    let unknown_code = remap_numeric_function(
                        &mut pairs,
                        ctx.interner,
                        data_key,
                        function_key,
                        fn_table,
                    );
                    if let Some(code) = unknown_code {
                        report_unknown_numeric_function(source_game, &code);
                        continue;
                    }

                    // --- Step 2: default CompareOperator to EqualTo ---
                    let has_data = pairs.iter().any(|(s, _)| *s == data_key);
                    let has_cv = pairs.iter().any(|(s, _)| *s == comparison_value_key);
                    let has_co = pairs.iter().any(|(s, _)| *s == compare_operator_key);
                    if has_data && has_cv && !has_co {
                        pairs.push((compare_operator_key, FieldValue::String(equal_to_sym)));
                    }

                    let mut cond_out = FieldValue::Struct(pairs);

                    // --- Step 3: remap FormKey strings ---
                    if do_remap {
                        deep_remap_formkey(&mut cond_out, ctx.interner, source_esm, target_esm);
                    }

                    cleaned.push(cond_out);
                }
                // Non-struct entries pass through unchanged.
                other => cleaned.push(other),
            }
        }

        *list = cleaned;
        Ok(())
    }
}

/// Check whether `Data.Function` is numeric. If it is:
/// - Look up the name in the fn_table.
/// - If found: replace the Function value with the name string.
/// - If NOT found: return the printable code (caller reports and drops it).
///
/// Returns `None` if Function is already a string (no-op) or if Data/Function
/// is absent.
fn remap_numeric_function(
    pairs: &mut Vec<(Sym, FieldValue)>,
    interner: &StringInterner,
    data_key: Sym,
    function_key: Sym,
    fn_table: &rustc_hash::FxHashMap<u16, String>,
) -> Option<String> {
    // Find the Data struct entry.
    let data_idx = match pairs.iter().position(|(s, _)| *s == data_key) {
        Some(i) => i,
        None => return None,
    };

    let data_pairs = match &mut pairs[data_idx].1 {
        FieldValue::Struct(dp) => dp,
        _ => return None,
    };

    // Find Function inside Data.
    let fn_idx = match data_pairs.iter().position(|(s, _)| *s == function_key) {
        Some(i) => i,
        None => return None,
    };

    let fn_val = &data_pairs[fn_idx].1;

    let code_opt: Option<u16> = match fn_val {
        FieldValue::Int(n) => u16::try_from(*n).ok(),
        FieldValue::Uint(n) => u16::try_from(*n).ok(),
        FieldValue::Float(f) if f.is_finite() && f.fract() == 0.0 => u16::try_from(*f as i64).ok(),
        // Already a named string — nothing to do.
        FieldValue::String(_) => return None,
        _ => return None,
    };

    let code = match code_opt {
        Some(c) => c,
        None => return Some(numeric_value_label(fn_val)),
    };

    match fn_table.get(&code) {
        Some(name) => {
            let name_sym = interner.intern(name);
            data_pairs[fn_idx].1 = FieldValue::String(name_sym);
            None
        }
        None => Some(code.to_string()),
    }
}

fn numeric_value_label(value: &FieldValue) -> String {
    match value {
        FieldValue::Int(value) => value.to_string(),
        FieldValue::Uint(value) => value.to_string(),
        FieldValue::Float(value) => value.to_string(),
        _ => "non-numeric".to_string(),
    }
}

fn unknown_numeric_function_message(source_game: &str, code: &str) -> String {
    format!("dropping {source_game} CTDA with unverified numeric function {code}")
}

fn report_unknown_numeric_function(source_game: &str, code: &str) {
    static REPORTED: OnceLock<Mutex<FxHashSet<String>>> = OnceLock::new();
    let key = format!("{}:{code}", source_game.to_ascii_lowercase());
    let reported = REPORTED.get_or_init(|| Mutex::new(FxHashSet::default()));
    let Ok(mut reported) = reported.lock() else {
        return;
    };
    if reported.insert(key) {
        eprintln!(
            "[translate_conditions] WARN: {}",
            unknown_numeric_function_message(source_game, code)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sym::StringInterner;
    use crate::translator::transforms::TransformCtx;
    use serde_json::json;

    /// Build a condition Struct with optional Data.Function (numeric or named).
    fn make_cond_with_named_fn(
        interner: &StringInterner,
        fn_name: &str,
        comparison_value: Option<f64>,
        compare_operator: Option<&str>,
        param1_fk: Option<&str>,
    ) -> FieldValue {
        let data_key = interner.intern("Data");
        let fn_key = interner.intern("Function");
        let fn_sym = interner.intern(fn_name);

        let mut data_pairs = vec![(fn_key, FieldValue::String(fn_sym))];
        if let Some(fk) = param1_fk {
            let p1_key = interner.intern("ParameterOneRecord");
            let fk_sym = interner.intern(fk);
            data_pairs.push((p1_key, FieldValue::String(fk_sym)));
        }

        let mut cond_pairs = vec![(data_key, FieldValue::Struct(data_pairs))];

        if let Some(cv) = comparison_value {
            let cv_key = interner.intern("ComparisonValue");
            cond_pairs.push((cv_key, FieldValue::Float(cv as f32)));
        }
        if let Some(co) = compare_operator {
            let co_key = interner.intern("CompareOperator");
            let co_sym = interner.intern(co);
            cond_pairs.push((co_key, FieldValue::String(co_sym)));
        }

        FieldValue::Struct(cond_pairs)
    }

    fn make_cond_with_numeric_fn(interner: &StringInterner, fn_code: i64) -> FieldValue {
        let data_key = interner.intern("Data");
        let fn_key = interner.intern("Function");
        let data_pairs = vec![(fn_key, FieldValue::Int(fn_code))];
        let cond_pairs = vec![(data_key, FieldValue::Struct(data_pairs))];
        FieldValue::Struct(cond_pairs)
    }

    fn make_cond_list(items: Vec<FieldValue>) -> FieldValue {
        FieldValue::List(items)
    }

    fn get_list_len(fv: &FieldValue) -> usize {
        match fv {
            FieldValue::List(l) => l.len(),
            _ => panic!("Expected List"),
        }
    }

    fn get_list(fv: &FieldValue) -> &Vec<FieldValue> {
        match fv {
            FieldValue::List(l) => l,
            _ => panic!("Expected List"),
        }
    }

    fn get_sym_str<'a>(interner: &'a StringInterner, sym: Sym) -> &'a str {
        interner.resolve(sym).unwrap_or("<unknown>")
    }

    // --- CTDA shape 1: named function, no CompareOperator needed ---
    #[test]
    fn translate_conditions_named_fn_passes_through() {
        let mut interner = StringInterner::new();
        let cond = make_cond_with_named_fn(&mut interner, "GetIsID", None, None, None);
        let mut value = make_cond_list(vec![cond]);
        let config = json!({ "source_esm": "SeventySix.esm", "target_esm": "Fallout4.esm" });
        let t = TranslateConditionsTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();
        assert_eq!(get_list_len(&value), 1, "Named-fn condition should survive");
    }

    // --- CTDA shape 2: numeric function code NOT in table → drop ---
    #[test]
    fn translate_conditions_unknown_numeric_fn_drops_condition() {
        let mut interner = StringInterner::new();
        // Code 875 is explicitly NOT in the table (per the YAML comments).
        let cond = make_cond_with_numeric_fn(&mut interner, 875);
        let mut value = make_cond_list(vec![cond]);
        let config = json!({});
        let t = TranslateConditionsTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();
        assert_eq!(
            get_list_len(&value),
            0,
            "Unknown numeric code should be dropped"
        );
    }

    // --- CTDA shape 3: numeric code that IS in the table → rewrite to name ---
    // (We inject a synthetic table lookup by building the map inline — the actual
    //  fo76 file is empty, but we test the remap_numeric_function helper directly.)
    #[test]
    fn remap_numeric_function_rewrites_known_code() {
        let mut interner = StringInterner::new();
        let data_key = interner.intern("Data");
        let fn_key = interner.intern("Function");
        let mut data_pairs = vec![(fn_key, FieldValue::Int(100))];
        let mut pairs = vec![(data_key, FieldValue::Struct(data_pairs.clone()))];

        let mut fn_table: rustc_hash::FxHashMap<u16, String> = rustc_hash::FxHashMap::default();
        fn_table.insert(100, "GetIsID".to_string());

        let unknown_code =
            remap_numeric_function(&mut pairs, &mut interner, data_key, fn_key, &fn_table);
        assert_eq!(unknown_code, None, "Known code should not be dropped");

        if let FieldValue::Struct(ref dp) = pairs[0].1 {
            if let FieldValue::String(sym) = dp[0].1 {
                assert_eq!(get_sym_str(&interner, sym), "GetIsID");
            } else {
                panic!("Expected String after remap");
            }
        }
    }

    #[test]
    fn skyrim_known_numeric_function_uses_verified_name() {
        let mut interner = StringInterner::new();
        let cond = make_cond_with_numeric_fn(&interner, 72);
        let mut value = make_cond_list(vec![cond]);
        let config = json!({ "source_game": "skyrimse" });
        let t = TranslateConditionsTransform;
        let mut ctx = TransformCtx {
            interner: &interner,
        };

        t.apply(&mut ctx, &mut value, &config).unwrap();

        let FieldValue::Struct(condition) = &get_list(&value)[0] else {
            panic!("expected condition struct");
        };
        let data_key = interner.intern("Data");
        let function_key = interner.intern("Function");
        let FieldValue::Struct(data) = &condition
            .iter()
            .find(|(key, _)| *key == data_key)
            .expect("Data")
            .1
        else {
            panic!("expected Data struct");
        };
        let FieldValue::String(function) = &data
            .iter()
            .find(|(key, _)| *key == function_key)
            .expect("Function")
            .1
        else {
            panic!("expected named function");
        };
        assert_eq!(interner.resolve(*function), Some("GetIsID"));
    }

    #[test]
    fn skyrim_unknown_numeric_function_reports_and_drops() {
        let mut interner = StringInterner::new();
        let cond = make_cond_with_numeric_fn(&interner, 875);
        let mut value = make_cond_list(vec![cond]);
        let config = json!({ "source_game": "skyrimse" });
        let t = TranslateConditionsTransform;
        let mut ctx = TransformCtx {
            interner: &interner,
        };

        t.apply(&mut ctx, &mut value, &config).unwrap();

        assert_eq!(get_list_len(&value), 0);
        assert_eq!(
            unknown_numeric_function_message("skyrimse", "875"),
            "dropping skyrimse CTDA with unverified numeric function 875"
        );
    }

    // --- CTDA shape 4: ComparisonValue present, CompareOperator absent → default EqualTo ---
    #[test]
    fn translate_conditions_defaults_compare_operator_to_equal_to() {
        let mut interner = StringInterner::new();
        let cond = make_cond_with_named_fn(
            &mut interner,
            "GetIsID",
            Some(1.0), // ComparisonValue present
            None,      // CompareOperator absent
            None,
        );
        let mut value = make_cond_list(vec![cond]);
        let config = json!({});
        let t = TranslateConditionsTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();

        let conditions = get_list(&value);
        assert_eq!(conditions.len(), 1);
        if let FieldValue::Struct(pairs) = &conditions[0] {
            let co_key = ctx.interner.intern("CompareOperator");
            let co = pairs.iter().find(|(s, _)| *s == co_key).map(|(_, v)| v);
            if let Some(FieldValue::String(sym)) = co {
                assert_eq!(get_sym_str(ctx.interner, *sym), "EqualTo");
            } else {
                panic!("CompareOperator should be set to EqualTo, got: {co:?}");
            }
        }
    }

    // --- CTDA shape 5: CompareOperator already present → not overridden ---
    #[test]
    fn translate_conditions_does_not_override_existing_compare_operator() {
        let mut interner = StringInterner::new();
        let cond = make_cond_with_named_fn(
            &mut interner,
            "GetIsID",
            Some(1.0),
            Some("GreaterThan"),
            None,
        );
        let mut value = make_cond_list(vec![cond]);
        let config = json!({});
        let t = TranslateConditionsTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();

        let conditions = get_list(&value);
        if let FieldValue::Struct(pairs) = &conditions[0] {
            let co_key = ctx.interner.intern("CompareOperator");
            let co = pairs.iter().find(|(s, _)| *s == co_key).map(|(_, v)| v);
            if let Some(FieldValue::String(sym)) = co {
                assert_eq!(get_sym_str(ctx.interner, *sym), "GreaterThan");
            }
        }
    }

    // --- CTDA shape 6: FormKey in ParameterOneRecord is remapped ---
    #[test]
    fn translate_conditions_remaps_esm_in_param_formkey() {
        let mut interner = StringInterner::new();
        let cond = make_cond_with_named_fn(
            &mut interner,
            "GetIsID",
            None,
            None,
            Some("001234@SeventySix.esm"),
        );
        let mut value = make_cond_list(vec![cond]);
        let config = json!({ "source_esm": "SeventySix.esm", "target_esm": "Fallout4.esm" });
        let t = TranslateConditionsTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();

        let conditions = get_list(&value);
        if let FieldValue::Struct(cond_pairs) = &conditions[0] {
            let data_key = ctx.interner.intern("Data");
            if let Some((_, FieldValue::Struct(dp))) =
                cond_pairs.iter().find(|(s, _)| *s == data_key)
            {
                let p1_key = ctx.interner.intern("ParameterOneRecord");
                if let Some((_, FieldValue::String(sym))) = dp.iter().find(|(s, _)| *s == p1_key) {
                    let s = get_sym_str(ctx.interner, *sym);
                    assert!(
                        s.contains("Fallout4.esm"),
                        "FormKey should be remapped: {s}"
                    );
                } else {
                    panic!("ParameterOneRecord not found or not a String");
                }
            }
        }
    }

    // --- CTDA shape 7: non-list value passes through unchanged ---
    #[test]
    fn translate_conditions_non_list_passes_through() {
        let mut interner = StringInterner::new();
        let mut value = FieldValue::Int(99);
        let config = json!({});
        let t = TranslateConditionsTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();
        assert_eq!(value, FieldValue::Int(99));
    }

    // --- CTDA shape 8: non-struct entry in list passes through ---
    #[test]
    fn translate_conditions_non_struct_entry_passes_through() {
        let mut interner = StringInterner::new();
        let mut value = FieldValue::List(vec![FieldValue::Int(42)]);
        let config = json!({});
        let t = TranslateConditionsTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();
        assert_eq!(get_list(&value), &[FieldValue::Int(42)]);
    }

    // --- CTDA shape 9: empty list stays empty ---
    #[test]
    fn translate_conditions_empty_list_stays_empty() {
        let mut interner = StringInterner::new();
        let mut value = FieldValue::List(vec![]);
        let config = json!({ "source_esm": "SeventySix.esm", "target_esm": "Fallout4.esm" });
        let t = TranslateConditionsTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();
        assert_eq!(get_list_len(&value), 0);
    }

    // --- CTDA shape 10: multiple conditions, mix of known-named and unknown-numeric ---
    #[test]
    fn translate_conditions_mixed_list_drops_unknown_keeps_named() {
        let mut interner = StringInterner::new();
        let named = make_cond_with_named_fn(&mut interner, "GetIsID", None, None, None);
        let unknown_numeric = make_cond_with_numeric_fn(&mut interner, 9999);
        let mut value = make_cond_list(vec![named, unknown_numeric]);
        let config = json!({});
        let t = TranslateConditionsTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();
        assert_eq!(
            get_list_len(&value),
            1,
            "Only named condition should survive"
        );
    }

    // --- CTDA shape 11: out-of-range negative numeric code → drop ---
    #[test]
    fn translate_conditions_negative_numeric_fn_drops_condition() {
        let mut interner = StringInterner::new();
        let cond = make_cond_with_numeric_fn(&mut interner, -1);
        let mut value = make_cond_list(vec![cond]);
        let config = json!({});
        let t = TranslateConditionsTransform;
        let mut ctx = TransformCtx {
            interner: &mut interner,
        };
        t.apply(&mut ctx, &mut value, &config).unwrap();
        assert_eq!(
            get_list_len(&value),
            0,
            "Negative code is out-of-range u16, should drop"
        );
    }
}
