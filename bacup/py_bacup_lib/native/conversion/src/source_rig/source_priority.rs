#[derive(Clone, Copy, Debug)]
pub(super) struct Contributor {
    pub active: bool,
    pub priority: u8,
    pub weight: f32,
    pub ease: f32,
}

pub(super) fn normalized_weights(inputs: &[Contributor]) -> Vec<f32> {
    let active = inputs.iter().enumerate().filter(|(_, input)| input.active).collect::<Vec<_>>();
    let mut result = vec![0.0; inputs.len()];
    if active.len() == 1 {
        result[active[0].0] = 1.0;
        return result;
    }
    let Some(high) = active.iter().map(|(_, input)| input.priority).max() else { return result };
    let next = active.iter().map(|(_, input)| input.priority).filter(|priority| *priority < high).max();
    let real = |input: &Contributor| input.weight * input.ease;
    let total = active.iter().map(|(_, input)| real(input)).sum::<f32>();
    let high_ease = active.iter().filter(|(_, input)| input.priority == high).map(|(_, input)| input.ease).fold(0.0f32, f32::max);
    if active.len() == 2 {
        if total <= 0.0 { return result; }
        if next.is_none() {
            for (index, input) in active { result[index] = real(input) / total; }
            return result;
        }
        if high_ease == 1.0 {
            for (index, input) in active { result[index] = if input.priority == high { 1.0 } else { 0.0 }; }
            return result;
        }
    }
    let numerator = |input: &Contributor| {
        if input.priority == high { high_ease * real(input) }
        else if Some(input.priority) == next { (1.0 - high_ease) * real(input) }
        else { 0.0 }
    };
    let denominator = active.iter().map(|(_, input)| numerator(input)).sum::<f32>();
    if denominator > 0.0 {
        for (index, input) in active { result[index] = numerator(input) / denominator; }
    }
    result
}

pub(super) struct ExpressionContributor {
    pub active: String,
    pub priority: String,
    pub weight: String,
    pub ease: String,
}

pub(super) struct ExpressionPlan {
    pub assignments: Vec<(String, String)>,
    pub weights: Vec<String>,
}

pub(super) fn expressions(prefix: &str, inputs: &[ExpressionContributor]) -> ExpressionPlan {
    let mut assignments = Vec::new();
    let mut assign = |suffix: String, expression: String| {
        let name = format!("{prefix}_{suffix}");
        assignments.push((name.clone(), expression));
        name
    };
    let mut active = Vec::new();
    let mut real = Vec::new();
    let mut high = assign("high0".into(), "-1".into());
    for (index, input) in inputs.iter().enumerate() {
        let enabled = assign(format!("active{index}"), format!("({}) * ({} >= 0)", input.active, input.priority));
        real.push(assign(format!("real{index}"), format!("{enabled} * ({}) * ({})", input.weight, input.ease)));
        let candidate = format!("(-1 + {enabled} * (({}) + 1))", input.priority);
        high = assign(format!("high{}", index + 1), select_max(&high, &candidate));
        active.push(enabled);
    }
    let mut next = assign("next0".into(), "-1".into());
    let mut high_ease = assign("ease0".into(), "0".into());
    for (index, input) in inputs.iter().enumerate() {
        let candidate = format!("(-1 + {} * ({} < {high}) * (({}) + 1))", active[index], input.priority, input.priority);
        next = assign(format!("next{}", index + 1), select_max(&next, &candidate));
        let ease = format!("({} * ({} == {high}) * ({}))", active[index], input.priority, input.ease);
        high_ease = assign(format!("ease{}", index + 1), select_max(&high_ease, &ease));
    }
    let sum = |items: &[String]| if items.is_empty() { "0".into() } else { items.join(" + ") };
    let count = assign("count".into(), sum(&active));
    let total = assign("total".into(), sum(&real));
    let numerators = inputs.iter().enumerate().map(|(index, input)| assign(format!("numerator{index}"), format!("({} == {high}) * {high_ease} * {} + ({} == {next}) * (1 - {high_ease}) * {}", input.priority, real[index], input.priority, real[index]))).collect::<Vec<_>>();
    let denominator = assign("denominator".into(), sum(&numerators));
    let weights = inputs.iter().enumerate().map(|(index, input)| {
        let general = format!("({denominator} > 0) * {} / ({denominator} + ({denominator} <= 0))", numerators[index]);
        let two = format!("{} * ({total} > 0) * (({next} < 0) * {} / ({total} + ({total} <= 0)) + ({next} >= 0) * (({high_ease} == 1) * ({} == {high}) + ({high_ease} < 1) * ({general})))", active[index], real[index], input.priority);
        assign(format!("weight{index}"), format!("({count} == 1) * {} + ({count} == 2) * ({two}) + ({count} > 2) * ({general})", active[index]))
    }).collect();
    ExpressionPlan { assignments, weights }
}

fn select_max(left: &str, right: &str) -> String {
    format!("(({left}) >= ({right})) * ({left}) + (({left}) < ({right})) * ({right})")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(priority: u8, weight: f32) -> Contributor { Contributor { active: true, priority, weight, ease: 1.0 } }

    #[test]
    fn source_priority_preserves_sparse_zero_weight_and_two_entry_rules() {
        assert_eq!(normalized_weights(&[input(60, 0.0)]), [1.0]);
        assert_eq!(normalized_weights(&[input(60, 0.0), input(22, 1.0)]), [1.0, 0.0]);
        assert_eq!(normalized_weights(&[input(60, 0.0), input(22, 1.0), input(22, 1.0)]), [0.0, 0.0, 0.0]);
        assert_eq!(normalized_weights(&[input(20, 0.5), input(20, 1.0), input(20, 1.0)]), [0.2, 0.4, 0.4]);
        assert_eq!(normalized_weights(&[input(20, 0.0), input(20, 1.0), input(20, 1.0)]), [0.0, 0.5, 0.5]);
        assert_eq!(normalized_weights(&[input(20, 1.0), input(22, 1.0), input(20, 1.0)]), [0.0, 1.0, 0.0]);
        assert_eq!(normalized_weights(&[input(20, 0.0), input(20, 0.0)]), [0.0, 0.0]);
    }

    #[test]
    fn source_priority_ease_only_admits_the_next_priority_level() {
        let mut high = input(60, 1.0);
        high.ease = 0.5;
        let weights = normalized_weights(&[high, input(22, 1.0), input(20, 1.0)]);
        assert!((weights[0] - 1.0 / 3.0).abs() < 1e-6);
        assert!((weights[1] - 2.0 / 3.0).abs() < 1e-6);
        assert_eq!(weights[2], 0.0);
    }

    #[test]
    fn emitted_source_priority_expressions_match_sparse_channel_weights() {
        use nif_core_native::expr::{MapContext, NifExpr, Value};
        let expressions = expressions("weights", &(0..4).map(|index| ExpressionContributor {
            active: format!("active{index}"), priority: format!("priority{index}"),
            weight: format!("weight{index}"), ease: format!("ease{index}"),
        }).collect::<Vec<_>>());
        let parsed = expressions.assignments.iter().map(|(name, expression)|
            (name, NifExpr::parse(expression).unwrap())).collect::<Vec<_>>();
        for active_bits in 0..16 {
            for pitch_weight in [0.0, 0.25, 0.5, 1.0] {
                for ease in [0.0, 0.5, 1.0] {
                    for priorities in [[20, 20, 20, 20], [60, 22, 22, 20], [20, 22, 60, 22]] {
                        let mut inputs = [input(0, 1.0); 4];
                        let mut context = MapContext::new();
                        for index in 0..4 {
                            inputs[index].active = active_bits & (1 << index) != 0;
                            inputs[index].priority = priorities[index];
                            if index == 0 { inputs[index].weight = pitch_weight; inputs[index].ease = ease; }
                            for (name, value) in [
                                ("active", if inputs[index].active { 1.0 } else { 0.0 }),
                                ("priority", inputs[index].priority as f64),
                                ("weight", inputs[index].weight as f64),
                                ("ease", inputs[index].ease as f64),
                            ] { context.0.insert(format!("{name}{index}"), Value::Float(value)); }
                        }
                        for (name, expression) in &parsed {
                            let value = expression.evaluate(&context).as_float();
                            assert!(value.is_finite(), "{name}: {}", expression.source);
                            context.0.insert((*name).clone(), Value::Float(value));
                        }
                        for (name, expected) in expressions.weights.iter().zip(normalized_weights(&inputs)) {
                            let actual = context.0[name].as_float();
                            assert!((actual - expected as f64).abs() < 1e-6, "{inputs:?}: {name}={actual}, expected {expected}");
                        }
                    }
                }
            }
        }
    }
}
