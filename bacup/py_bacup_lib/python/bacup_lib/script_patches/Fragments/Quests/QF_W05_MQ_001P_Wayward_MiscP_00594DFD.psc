Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True)
    If Alias_owningPlayer.GetRef()
        Alias_owningPlayer.GetRef().SetValue(W05_MQ_001P_Wayward_PlayerStartedMiscPointer, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction
