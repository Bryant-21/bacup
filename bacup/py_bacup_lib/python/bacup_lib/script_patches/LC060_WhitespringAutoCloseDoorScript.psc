Function StartAutoCloseTimer()
    Float closeDelay = AutoCloseDelay
    If closeDelay <= 0.0
        closeDelay = DefaultAutoCloseDelay.GetValue()
    EndIf
    If closeDelay > 0.0
        StartTimer(closeDelay, AutoCloseTimerID)
    EndIf
EndFunction

Event OnLoad()
    If GetOpenState() == 1
        StartAutoCloseTimer()
    EndIf
EndEvent

Event OnOpen(ObjectReference akActionRef)
    StartAutoCloseTimer()
EndEvent

Event OnClose(ObjectReference akActionRef)
    CancelTimer(AutoCloseTimerID)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == AutoCloseTimerID && GetOpenState() == 1
        SetOpen(False)
    EndIf
EndEvent
