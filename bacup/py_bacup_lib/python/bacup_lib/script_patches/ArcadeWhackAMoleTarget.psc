; Replace server score messages with direct local controller calls.

Function UpdateScore(Actor akSendingPlayer)
    hits = hits + 1
    If gameController != None
        gameController.RegisterMoleHit(scorePerHit)
    EndIf
EndFunction

Function BecomeVulnerable(Actor akSendingPlayer)
    vulnerable = True
EndFunction

Function SendRMIToServer(String functionName, Var[] arguments)
    ; Deliberate local no-op: the useful calls are handled directly above.
EndFunction

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, Bool abCritAttack, String asMaterialName)
    If !vulnerable
        Return
    EndIf
    vulnerable = False
    PlayHitSFX()
    UpdateScore(akAggressor as Actor)
EndEvent

Function PlayHitSFX()
    If HitSound != None
        HitSound.Play(Self)
    EndIf
EndFunction
