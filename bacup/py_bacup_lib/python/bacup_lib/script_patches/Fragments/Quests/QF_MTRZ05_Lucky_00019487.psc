Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
    MTRZ05_MinerCheckpointValue.SetValue(100.0)
EndFunction

Function Fragment_Stage_0255_Item_00()
    SetObjectiveCompleted(10, True)
    Alias_MTRZ05_MiningNode.GetReference().Disable()
    CompleteQuest()
    Stop()
EndFunction
