Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || MyQuestTargetValue <= 0
        Return
    EndIf
    MSiloPersonalQuestScript personal = MSiloPersonal as MSiloPersonalQuestScript
    If !MSiloPersonal.IsRunning()
        personal.EnsureSiloStarted(Game.GetPlayer().GetCurrentLocation())
    EndIf
    If !MSiloPersonal.IsRunning()
        Return
    EndIf
    personal.TryToSetStage(MyQuestTargetValue)
EndEvent
