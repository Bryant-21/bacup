Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    TriggerSpellUse = TriggerSpell
    If TriggerSpellUse == None
        Return
    EndIf

    If TriggerSpellDelay > 0.0
        StartTimer(TriggerSpellDelay, CONST_TriggerSpellTimerID)
    Else
        TriggerSpellUse.Cast(Self, akActionRef)
    EndIf
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        CancelTimer(CONST_TriggerSpellTimerID)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == CONST_TriggerSpellTimerID && TriggerSpellUse != None
        TriggerSpellUse.Cast(Self, Game.GetPlayer())
    EndIf
EndEvent
