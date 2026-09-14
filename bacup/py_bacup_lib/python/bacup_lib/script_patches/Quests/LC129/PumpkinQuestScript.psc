Function TryTurnInPumpkins()
    Actor playerRef = Alias_QuestPlayer.GetActorReference()
    If playerRef == None || GetStage() < 300 || GetStage() >= 600
        Return
    EndIf
    If playerRef.GetItemCount(PumpkinVegetable) >= 10
        SetStage(600)
    EndIf
EndFunction
