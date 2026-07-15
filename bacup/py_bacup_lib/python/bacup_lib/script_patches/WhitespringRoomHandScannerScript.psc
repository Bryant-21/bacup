Function ScanRoomDoor(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    If mySyncDoor == None
        mySyncDoor = GetLinkedRef(LinkedRefToActivate)
    EndIf

    If mySyncDoor == None || mySyncDoor.IsLocked()
        GoToState("ScanBlueToRed")
        Utility.Wait(1.0)
        GoToState("red")
    Else
        GoToState("ScanBlueToGreen")
        mySyncDoor.SetOpen(True)
        Utility.Wait(1.0)
        GoToState("Green")
    EndIf
EndFunction

Event OnActivate(ObjectReference akActionRef)
    ScanRoomDoor(akActionRef)
EndEvent
