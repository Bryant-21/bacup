Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && MTN_MQ_PlayerOn3rdFloorValue != None
        akActionRef.SetValue(MTN_MQ_PlayerOn3rdFloorValue, 1.0)
    EndIf
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && MTN_MQ_PlayerOn3rdFloorValue != None
        akActionRef.SetValue(MTN_MQ_PlayerOn3rdFloorValue, 0.0)
    EndIf
EndEvent
