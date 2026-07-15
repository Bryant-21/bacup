Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && FF05_Balance != None && FF05_Balance.IsRunning() && FF05_Balance_SensorMessage != None
        FF05_Balance_SensorMessage.Show()
    EndIf
EndEvent
