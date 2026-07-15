Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && EN02_PlayerInExamRoomValue != None
        akActionRef.SetValue(EN02_PlayerInExamRoomValue, 1.0)
    EndIf
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && EN02_PlayerInExamRoomValue != None
        akActionRef.SetValue(EN02_PlayerInExamRoomValue, 0.0)
    EndIf
EndEvent
