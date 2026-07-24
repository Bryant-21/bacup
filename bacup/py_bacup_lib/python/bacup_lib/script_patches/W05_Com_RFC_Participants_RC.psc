Event OnCombatStateChanged(ObjectReference akSenderRef, Actor akTarget, int aeCombatState)
    If akTarget != None && CurrentPlayerParticipants != None
        CurrentPlayerParticipants.AddRef(akTarget)
    EndIf
EndEvent
