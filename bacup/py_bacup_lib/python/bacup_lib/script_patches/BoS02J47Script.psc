Function ReconcileJ47(ObjectReference akDeadRef = None)
    If !pBoS02_J47IsAlive
        Return
    EndIf
    Int index = 0
    While index < GetCount()
        Actor candidate = GetAt(index) as Actor
        If candidate && candidate != akDeadRef && !candidate.IsDead()
            pBoS02_J47IsAlive.SetValue(1.0)
            Return
        EndIf
        index += 1
    EndWhile
    pBoS02_J47IsAlive.SetValue(0.0)
EndFunction

Event OnAliasInit()
    ReconcileJ47()
EndEvent

Event OnLoad(ObjectReference akSenderRef)
    ReconcileJ47()
EndEvent

Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    ReconcileJ47(akSenderRef)
EndEvent

Event OnReset(ObjectReference akSenderRef)
    ReconcileJ47()
EndEvent

Event OnAliasShutdown()
    If pBoS02_J47IsAlive
        pBoS02_J47IsAlive.SetValue(0.0)
    EndIf
EndEvent
