; Score a locally linked bottle-blaster controller when its destructible target
; is hit. Target placement and ticket payout remain server-owned.

; FO4 has no GetDestructibleHealthPercent; the destruction stage (-1 when undamaged)
; stands in for damage taken, a quarter of the target per stage, capped at the full target.
Int Function CalculateScore(Actor akAttacker)
    Float damagePercentDone = Math.Min(Math.Max(GetCurrentDestructionStage() as Float, 1.0) * 0.25, 1.0)
    Float strengthFactor = akAttacker.GetValue(StrengthAV) * StrengthScoreDampener
    Float rawScore = baseScore as Float * (damagePercentDone * 10.0 * DamageScoreDampener) * strengthFactor
    Return Math.Min(Math.Floor(rawScore) as Float, MaxScore as Float) as Int
EndFunction

Event OnLoad()
    parentScript = GetLinkedRef() as ArcadeBottleBlaster
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String asMaterialName)
    Actor attacker = akAggressor as Actor
    If attacker != None && parentScript != None
        Int earnedScore = CalculateScore(attacker)
        parentScript.score = parentScript.score + earnedScore
        parentScript.PlayHitAnim(parentScript.score)
    EndIf
EndEvent
