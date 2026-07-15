Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    ObjectReference linkedDoor = GetLinkedRef(DoorLinkKeyword)
    If linkedDoor == None
        Return
    EndIf

    Int doorState = linkedDoor.GetOpenState()
    If doorState == 1 || doorState == 2
        linkedDoor.SetOpen(False)
    Else
        linkedDoor.SetOpen(True)
    EndIf

    BlockActivation(True, False)
    If UseTimerCheck && TimerInterval > 0.0
        StartTimer(TimerInterval, iDoorTimerID)
    Else
        StartTimer(0.5, iActivateLockID)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iDoorTimerID
        ObjectReference linkedDoor = GetLinkedRef(DoorLinkKeyword)
        If linkedDoor != None
            Int doorState = linkedDoor.GetOpenState()
            If doorState == 2 || doorState == 4
                StartTimer(0.5, iDoorTimerID)
                Return
            EndIf
        EndIf
        BlockActivation(False, False)
    ElseIf aiTimerID == iActivateLockID
        BlockActivation(False, False)
    EndIf
EndEvent
