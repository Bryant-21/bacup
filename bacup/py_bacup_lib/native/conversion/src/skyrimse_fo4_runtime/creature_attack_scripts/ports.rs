use std::collections::BTreeSet;

use papyrus_core::ast::{Expr, FunctionDef, LiteralValue, Parameter, Pos, ScriptNode, Stmt};

pub(super) const SCRIPT_PORTS: &[(&str, &str)] = &[
    ("voicepusheffectscript", "B21_SkyrimVoicePushEffect"),
    ("magicabsorbfxscript", "B21_SkyrimAbsorbFXEffect"),
    (
        "AlduinInvulnerabilityEffectScript",
        "B21_SkyrimAlduinInvulnerability",
    ),
    ("atronachfrostscript", "B21_SkyrimAtronachFrost"),
    ("DLC1ChaurusFlyerFXSCRIPT", "B21_SkyrimChaurusFlyerFX"),
    ("DLC1DeerFXGlowScript", "B21_SkyrimDeerFXGlow"),
    ("DLC1FXDeathHoundSCRIPT", "B21_SkyrimDeathHoundFX"),
    ("DLC1FXDwarvenCenturionSCRIPT", "B21_SkyrimDLC1CenturionFX"),
    (
        "DLC1GargoyleBruteHandFXScript",
        "B21_SkyrimGargoyleBruteHandFX",
    ),
    ("DLC1SabreCatFXGlowScript", "B21_SkyrimSabreCatFXGlow"),
    ("DLC1SoulCairnCreatureFX", "B21_SkyrimSoulCairnCreatureFX"),
    ("DLC2AshSpawnConstantFX", "B21_SkyrimAshSpawnConstantFX"),
    (
        "DLC2AshSpawnDisintegrateDeathFX",
        "B21_SkyrimAshSpawnDeathFX",
    ),
    (
        "DLC2dunKarstaagIceWraithParticles",
        "B21_SkyrimKarstaagIceWraithFX",
    ),
    (
        "DLC2FXInsturmentsAttachSCRIPT",
        "B21_SkyrimInstrumentsAttachFX",
    ),
    ("DLC2FXScrib", "B21_SkyrimScribFX"),
    ("DragonPriestActorScript", "B21_SkyrimDragonPriestActor"),
    ("DraugrFXScript", "B21_SkyrimDraugrFX"),
    ("FlameAtronachEffectScript", "B21_SkyrimFlameAtronachFX"),
    ("FXAtronachStormSCRIPT", "B21_SkyrimAtronachStormFX"),
    ("FXChaurusScript", "B21_SkyrimChaurusFX"),
    ("FXDragonBloodDamageScript", "B21_SkyrimDragonBloodDamageFX"),
    ("FxDraugrMagicScript", "B21_SkyrimDraugrMagicFX"),
    ("FXDwarvenCenturionSCRIPT", "B21_SkyrimCenturionFX"),
    ("FXDwarvenSphereScript", "B21_SkyrimDwarvenSphereFX"),
    ("FXDwarvenSpiderSCRIPT", "B21_SkyrimDwarvenSpiderFX"),
    ("FXMG07DogScript", "B21_SkyrimMG07DogFX"),
    ("FXSkeletonNecroScript", "B21_SkyrimSkeletonNecroFX"),
    ("FXWitchlightAttachSCRIPT", "B21_SkyrimWitchlightAttachFX"),
    ("icewraithparticlesscript", "B21_SkyrimIceWraithParticles"),
    (
        "magicAttachAshPileOnDeath",
        "B21_SkyrimAttachAshPileOnDeath",
    ),
    (
        "magiceffectshadersonendscript",
        "B21_SkyrimEffectShadersOnEnd",
    ),
    ("magicimodbeginloopend", "B21_SkyrimImodBeginLoopEnd"),
    ("MagicImodOnPlayerHitScript", "B21_SkyrimImodOnPlayerHit"),
    ("magicNightEyeScript", "B21_SkyrimNightEye"),
    (
        "MagicPlayEffectShaderOnHitScript",
        "B21_SkyrimEffectShaderOnHit",
    ),
    ("magicProjectileStormScript", "B21_SkyrimProjectileStorm"),
    ("magicSetActorAlphaScript", "B21_SkyrimSetActorAlpha"),
    ("MG01FireEffectScript", "B21_SkyrimMG01FireEffect"),
    ("MG01QuestScript", "B21_SkyrimMG01Quest"),
    ("NoFlightAbilityScript", "B21_SkyrimNoFlightAbility"),
    ("ReanimateAshPile", "B21_SkyrimReanimateAshPile"),
    ("sprigganCallAnimalsAOE", "B21_SkyrimSprigganCallAnimals"),
    ("SprigganFXSCRIPT", "B21_SkyrimSprigganFX"),
    ("SprigganSwarmActorSCRIPT", "B21_SkyrimSprigganSwarmActor"),
    ("wispActorScript", "B21_SkyrimWispActor"),
    ("WispShadeActorScript", "B21_SkyrimWispShadeActor"),
];

fn name(value: &str) -> Expr {
    Expr::NameExpr {
        name: value.into(),
        pos: Pos::ZERO,
    }
}

pub(super) fn actor_values(target_script: &str) -> &'static [&'static str] {
    match target_script {
        "B21_SkyrimAtronachFrost"
        | "B21_SkyrimDeerFXGlow"
        | "B21_SkyrimDeathHoundFX"
        | "B21_SkyrimSabreCatFXGlow"
        | "B21_SkyrimAshSpawnConstantFX"
        | "B21_SkyrimScribFX"
        | "B21_SkyrimAtronachStormFX"
        | "B21_SkyrimFlameAtronachFX"
        | "B21_SkyrimDragonBloodDamageFX" => &["Health"],
        "B21_SkyrimSprigganCallAnimals" => &["Aggression", "Confidence"],
        "B21_SkyrimSprigganFX" => &["Variable07"],
        "B21_SkyrimWispActor" => &["Health", "Magicka", "Variable06", "Variable07"],
        _ => &[],
    }
}

fn invoke(method: &str, args: Vec<Expr>) -> Stmt {
    Stmt::ExprStmt {
        expr: Expr::DotCallExpr {
            object: Box::new(name("Self")),
            method: method.into(),
            args,
            arg_names: Vec::new(),
            pos: Pos::ZERO,
        },
        pos: Pos::ZERO,
    }
}

fn get_target() -> Expr {
    Expr::DotCallExpr {
        object: Box::new(name("Self")),
        method: "GetTargetActor".into(),
        args: Vec::new(),
        arg_names: Vec::new(),
        pos: Pos::ZERO,
    }
}

fn parameter(value: &str, ty: &str) -> Parameter {
    Parameter {
        name: value.into(),
        ty: ty.into(),
        ..Default::default()
    }
}

fn map_expr(expr: &mut Expr, change: &mut impl FnMut(&mut Expr)) {
    match expr {
        Expr::DotExpr { object, .. } => map_expr(object, change),
        Expr::DotCallExpr { object, args, .. } => {
            map_expr(object, change);
            for arg in args {
                map_expr(arg, change);
            }
        }
        Expr::CallExpr { args, .. } => {
            for arg in args {
                map_expr(arg, change);
            }
        }
        Expr::BinaryExpr { left, right, .. } => {
            map_expr(left, change);
            map_expr(right, change);
        }
        Expr::UnaryExpr { operand, .. } => map_expr(operand, change),
        Expr::CastExpr { expr, .. } => map_expr(expr, change),
        Expr::ArrayAccessExpr { array, index, .. } => {
            map_expr(array, change);
            map_expr(index, change);
        }
        Expr::NewArrayExpr { size, .. } => map_expr(size, change),
        _ => {}
    }
    change(expr);
}

fn map_body(body: &mut [Stmt], change: &mut impl FnMut(&mut Expr)) {
    for stmt in body {
        match stmt {
            Stmt::ExprStmt { expr, .. } => map_expr(expr, change),
            Stmt::AssignStmt { target, value, .. } => {
                map_expr(target, change);
                map_expr(value, change);
            }
            Stmt::ReturnStmt { value, .. } | Stmt::LocalVarStmt { value, .. } => {
                if let Some(value) = value {
                    map_expr(value, change);
                }
            }
            Stmt::WhileStmt {
                condition, body, ..
            } => {
                map_expr(condition, change);
                map_body(body, change);
            }
            Stmt::IfStmt {
                condition,
                body,
                elseif_clauses,
                else_body,
                ..
            } => {
                map_expr(condition, change);
                map_body(body, change);
                for clause in elseif_clauses {
                    map_expr(&mut clause.condition, change);
                    map_body(&mut clause.body, change);
                }
                map_body(else_body, change);
            }
        }
    }
}

fn add_fragment(ast: &mut ScriptNode, source: &str) -> Result<(), String> {
    let parsed = papyrus_core::parser::parse_script(&format!(
        "Scriptname Fragment extends ActiveMagicEffect\n{source}"
    ));
    if !parsed.errors.is_empty() {
        return Err(format!(
            "invalid creature compatibility fragment: {:?}",
            parsed.errors
        ));
    }
    let fragment = parsed
        .ast
        .ok_or("missing creature compatibility fragment")?;
    ast.variables.extend(fragment.variables);
    ast.functions.extend(fragment.functions);
    ast.events.extend(fragment.events);
    Ok(())
}

pub(super) fn adapt(ast: &mut ScriptNode) -> Result<(), String> {
    let all_events = ast
        .events
        .iter()
        .chain(ast.states.iter().flat_map(|state| &state.events));
    let event_names: BTreeSet<_> = all_events
        .map(|event| event.name.to_ascii_lowercase())
        .collect();
    let hit = event_names.contains("onhit");
    let update = event_names.contains("onupdate");
    let los = event_names.contains("ongainlos") || event_names.contains("onlostlos");
    ast.functions.retain(|function| {
        !matches!(
            function.name.to_ascii_lowercase().as_str(),
            "getstate" | "gotostate"
        )
    });
    for events in
        std::iter::once(&mut ast.events).chain(ast.states.iter_mut().map(|state| &mut state.events))
    {
        for event in events {
            match event.name.to_ascii_lowercase().as_str() {
                "onhit" if event.params.len() == 7 => {
                    event
                        .params
                        .insert(0, parameter("B21HitTarget", "ObjectReference"));
                    event.params.push(parameter("B21HitMaterial", "String"));
                    event
                        .body
                        .insert(0, invoke("RegisterForHitEvent", vec![get_target()]));
                }
                "oneffectstart" if hit => {
                    event
                        .body
                        .insert(0, invoke("RegisterForHitEvent", vec![get_target()]));
                }
                "onbeginstate" | "onendstate" if event.params.is_empty() => {
                    event
                        .params
                        .push(parameter("B21PreviousOrNextState", "String"));
                }
                _ => {}
            }
        }
    }
    if hit
        && !ast
            .events
            .iter()
            .any(|event| event.name.eq_ignore_ascii_case("OnHit"))
    {
        add_fragment(
            ast,
            "Event OnHit(ObjectReference B21HitTarget, ObjectReference B21Aggressor, Form B21Source, Projectile B21Projectile, Bool B21Power, Bool B21Sneak, Bool B21Bash, Bool B21Blocked, String B21Material)\nRegisterForHitEvent(GetTargetActor())\nEndEvent",
        )?;
    }
    let mut actor_values = BTreeSet::new();
    let mut error = None;
    let mut calls = |expr: &mut Expr| {
        let Expr::DotCallExpr { method, args, .. } = expr else {
            return;
        };
        match method.to_ascii_lowercase().as_str() {
            "registerforsingleupdate" => {
                *method = "StartTimer".into();
                args.push(Expr::LiteralExpr {
                    value: LiteralValue::Int(0),
                    ty: "int".into(),
                    pos: Pos::ZERO,
                });
            }
            "registerforsinglelosgain" => *method = "B21RegisterForSingleLOSGain".into(),
            "registerforsingleloslost" => *method = "B21RegisterForSingleLOSLost".into(),
            "getav"
            | "getactorvalue"
            | "getbaseav"
            | "getbaseactorvalue"
            | "setav"
            | "setactorvalue"
            | "damageactorvalue"
            | "getactorvaluepercentage" => {
                let target = match method.to_ascii_lowercase().as_str() {
                    "getav" | "getactorvalue" => "GetValue",
                    "getbaseav" | "getbaseactorvalue" => "GetBaseValue",
                    "setav" | "setactorvalue" => "SetValue",
                    "damageactorvalue" => "DamageValue",
                    _ => "GetValuePercentage",
                };
                if let Some(Expr::LiteralExpr {
                    value: LiteralValue::Str(value),
                    ..
                }) = args.first()
                {
                    let canonical = match value.to_ascii_lowercase().as_str() {
                        "health" => "Health",
                        "confidence" => "Confidence",
                        "aggression" => "Aggression",
                        "variable06" => "Variable06",
                        "variable07" => "Variable07",
                        "magicka" => "Magicka",
                        _ => {
                            error =
                                Some(format!("unsupported creature script actor value {value}"));
                            return;
                        }
                    };
                    actor_values.insert(canonical);
                    args[0] = name(&format!("B21AV_{canonical}"));
                    *method = target.into();
                } else {
                    error = Some(format!("dynamic creature actor value in {method}"));
                }
            }
            _ => {}
        }
    };
    for function in &mut ast.functions {
        map_body(&mut function.body, &mut calls);
    }
    for event in &mut ast.events {
        map_body(&mut event.body, &mut calls);
    }
    for state in &mut ast.states {
        for function in &mut state.functions {
            map_body(&mut function.body, &mut calls);
        }
        for event in &mut state.events {
            map_body(&mut event.body, &mut calls);
        }
    }
    if let Some(error) = error {
        return Err(error);
    }
    let expected: BTreeSet<_> = self::actor_values(&ast.name).iter().copied().collect();
    if actor_values != expected {
        return Err(format!(
            "creature script {} actor values changed: expected {expected:?}, found {actor_values:?}",
            ast.name
        ));
    }
    for value in actor_values {
        ast.properties.push(papyrus_core::ast::PropertyDef {
            name: format!("B21AV_{value}"),
            ty: "ActorValue".into(),
            flags: vec!["Auto".into()],
            ..Default::default()
        });
    }
    for (events, functions) in std::iter::once((&mut ast.events, &mut ast.functions)).chain(
        ast.states
            .iter_mut()
            .map(|state| (&mut state.events, &mut state.functions)),
    ) {
        let mut retained = Vec::new();
        for event in std::mem::take(events) {
            if matches!(
                event.name.to_ascii_lowercase().as_str(),
                "onupdate" | "ongainlos" | "onlostlos"
            ) {
                functions.push(FunctionDef {
                    name: event.name,
                    params: event.params,
                    body: event.body,
                    ..Default::default()
                });
            } else {
                retained.push(event);
            }
        }
        *events = retained;
    }
    if update || los {
        let mut timer = "Event OnTimer(Int B21TimerID)\n".to_string();
        if update {
            timer.push_str("If B21TimerID == 0\nSelf.OnUpdate()\nEndIf\n");
        }
        if los {
            timer.push_str("If B21TimerID == 1\nSelf.B21CheckLOS()\nEndIf\n");
        }
        timer.push_str("EndEvent\n");
        add_fragment(ast, &timer)?;
        if ast
            .parent
            .as_deref()
            .is_some_and(|parent| parent.eq_ignore_ascii_case("ActiveMagicEffect"))
        {
            let cleanup = papyrus_core::parser::parse_script("Scriptname Cleanup extends ActiveMagicEffect\nEvent OnEffectFinish(Actor target, Actor caster)\nCancelTimer(0)\nCancelTimer(1)\nEndEvent\n").ast.unwrap().events.remove(0);
            let mut existing = false;
            for event in ast
                .events
                .iter_mut()
                .chain(ast.states.iter_mut().flat_map(|state| &mut state.events))
            {
                if event.name.eq_ignore_ascii_case("OnEffectFinish") {
                    existing = true;
                    event.body.splice(0..0, cleanup.body.clone());
                }
            }
            if !existing {
                ast.events.push(cleanup);
            }
        }
    }
    if los {
        add_fragment(ast, include_str!("line_of_sight.psc"))?;
    }
    Ok(())
}
