; Score a locally linked bottle-blaster controller when its destructible target
; is hit. Target placement and ticket payout remain server-owned.

Event OnLoad()
    parentScript = GetLinkedRef() as ArcadeBottleBlaster
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, Bool abCritAttack, String asMaterialName)
    Actor attacker = akAggressor as Actor
    If attacker != None && parentScript != None
        Int earnedScore = CalculateScore(attacker)
        parentScript.score = parentScript.score + earnedScore
        parentScript.PlayHitAnim(parentScript.score)
    EndIf
EndEvent
