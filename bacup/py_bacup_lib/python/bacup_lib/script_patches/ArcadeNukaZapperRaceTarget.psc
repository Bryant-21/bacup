; A linked rocket is the only bound/local race payload. Each valid hit increases
; that rocket's animation speed by the configured amount.

Event OnLoad()
    If myRocket == None
        myRocket = GetLinkedRef() as ArcadeNukaZapperRaceRocket
    EndIf
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, Bool abCritAttack, String asMaterialName)
    If myRocket != None
        IncreaseSpeed(myRocket)
    EndIf
EndEvent
