Function RegisterGuardsForHits()
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsRunning()
        CancelTimer(51005)
        Return
    EndIf
    Int index = 0
    While index < GetCount()
        Actor guard = GetAt(index) as Actor
        If guard != None && !guard.IsDead()
            RegisterForHitEvent(guard)
        EndIf
        index += 1
    EndWhile
    ; Respawned guards join the collection at any time and FO4 only reports hits for
    ; references that were registered, so keep the registration current.
    StartTimer(5.0, 51005)
EndFunction

Event OnAliasInit()
    RegisterGuardsForHits()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 51005
        RegisterGuardsForHits()
    EndIf
EndEvent

Event OnHit(ObjectReference akSenderRef, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String asMaterial)
    Actor guard = akSenderRef as Actor
    If guard == None || CB02_LastAttackedExpiryAV == None || GameDaysPassed == None
        Return
    EndIf
    ; The bucket guards' packages read this expiry stamp to decide how long they chase
    ; whoever hit them instead of returning to the candy bucket.
    guard.SetValue(CB02_LastAttackedExpiryAV, GameDaysPassed.GetValue() + 0.01)
    guard.EvaluatePackage()
EndEvent

Event OnAliasShutdown()
    CancelTimer(51005)
    UnregisterForAllHitEvents()
EndEvent
