Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || MyQuestTargetValue <= 0
        Return
    EndIf
    If !MSiloPersonal.IsRunning()
        MSiloPersonal.Start()
    EndIf
    MSiloPersonalQuestScript personal = MSiloPersonal as MSiloPersonalQuestScript
    personal.TryToSetStage(MyQuestTargetValue)
EndEvent
