Event OnCombatStateChanged(ObjectReference akSenderRef, Actor akTarget, int aeCombatState)
    If aeCombatState != 0 && RecentlyAttackedAlias != None && akSenderRef != None
        RecentlyAttackedAlias.ForceRefTo(akSenderRef)
    EndIf
EndEvent
