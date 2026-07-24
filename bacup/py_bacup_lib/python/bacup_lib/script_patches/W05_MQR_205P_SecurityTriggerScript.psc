Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || SecurityRoomCollision == None
        Return
    EndIf

    ObjectReference collisionRef = SecurityRoomCollision.GetReference()
    If collisionRef != None
        collisionRef.Disable()
    EndIf
EndEvent
