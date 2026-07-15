Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    SpellToUse = TriggerSpell
    If SpellToUse == None
        Return
    EndIf

    If TriggerSpellDelay > 0.0
        StartTimer(TriggerSpellDelay, CONST_TriggerSpellTimerID)
    Else
        SpellToUse.Cast(Self, akActionRef)
    EndIf
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        CancelTimer(CONST_TriggerSpellTimerID)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == CONST_TriggerSpellTimerID && SpellToUse != None
        SpellToUse.Cast(Self, Game.GetPlayer())
    EndIf
EndEvent
