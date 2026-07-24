Event OnOpen(ObjectReference akActionRef)
    CancelTimer(AutoCloseTimerID)

    Float delay = AutoCloseDelay
    If delay <= 0.0 && DefaultAutoCloseDelay != None
        delay = DefaultAutoCloseDelay.GetValue()
    EndIf

    If delay > 0.0
        StartTimer(delay, AutoCloseTimerID)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == AutoCloseTimerID
        Int openState = GetOpenState()
        If openState == 1 || openState == 2
            SetOpen(False)
        EndIf
    EndIf
EndEvent
