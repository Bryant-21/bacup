Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
    Actor playerRef = Game.GetPlayer()
    If Alias_MTRZ05_Player != None && Alias_MTRZ05_Player.GetActorReference() != None
        playerRef = Alias_MTRZ05_Player.GetActorReference()
    EndIf
    If playerRef != None && MTRZ05_MinerCheckpointValue != None
        playerRef.SetValue(MTRZ05_MinerCheckpointValue, 100.0)
    EndIf
EndFunction

Function Fragment_Stage_0255_Item_00()
    SetObjectiveCompleted(10, True)
    If Alias_MTRZ05_MiningNode != None && Alias_MTRZ05_MiningNode.GetReference() != None
        Alias_MTRZ05_MiningNode.GetReference().Disable()
    EndIf
    CompleteQuest()
    Stop()
EndFunction
