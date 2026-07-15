State Ready
    Event OnActivate(ObjectReference akActionRef)
        If akActionRef != Game.GetPlayer()
            Return
        EndIf

        If akActionRef.GetItemCount(IDCard) < 1
            BroadcastFailure()
            Return
        EndIf

        ObjectReference linkedRef = GetLinkedRef(myLinkedRefToActivate)
        If linkedRef == None
            Return
        EndIf

        GoToState("busy")
        linkedRef.Activate(akActionRef)
        If iDoorOpenTimerLength > 0
            StartTimer(iDoorOpenTimerLength, iDoorOpenTimerID)
        Else
            GoToState("Ready")
        EndIf
    EndEvent
EndState

State busy
    Event OnActivate(ObjectReference akActionRef)
    EndEvent

    Event OnTimer(Int aiTimerID)
        If aiTimerID != iDoorOpenTimerID
            Return
        EndIf

        ObjectReference linkedRef = GetLinkedRef(myLinkedRefToActivate)
        If linkedRef != None
            linkedRef.Activate(Game.GetPlayer())
        EndIf
        GoToState("Ready")
    EndEvent
EndState
