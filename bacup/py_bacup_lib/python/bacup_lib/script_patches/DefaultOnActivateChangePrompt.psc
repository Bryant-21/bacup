Event OnActivate(ObjectReference akActionRef)
    ; TODO
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    SetPrompt_New()
    If Delay > 0.0
        StartTimer(Delay, 1)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1
        SetPrompt_Old()
    EndIf
EndEvent
