Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    CancelTimer(CloseDoorTimerID)
    ObjectReference elevatorDoor = GetLinkedRef(None)
    If elevatorDoor != None
        elevatorDoor.SetOpen(True)
        PlayElevatorDingSound()
    EndIf
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    If CloseDoorTime > 0.0
        StartTimer(CloseDoorTime, CloseDoorTimerID)
    Else
        ObjectReference elevatorDoor = GetLinkedRef(None)
        If elevatorDoor != None
            elevatorDoor.SetOpen(False)
        EndIf
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == CloseDoorTimerID
        ObjectReference elevatorDoor = GetLinkedRef(None)
        If elevatorDoor != None
            elevatorDoor.SetOpen(False)
        EndIf
    EndIf
EndEvent
