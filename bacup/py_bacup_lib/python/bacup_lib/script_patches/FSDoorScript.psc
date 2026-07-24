Event OnOpen(ObjectReference akActionRef)
    StartTimer(DoorTimer, 1)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1
        Int openState = GetOpenState()
        If openState == 1 || openState == 2
            SetOpen(False)
        EndIf
    EndIf
EndEvent
