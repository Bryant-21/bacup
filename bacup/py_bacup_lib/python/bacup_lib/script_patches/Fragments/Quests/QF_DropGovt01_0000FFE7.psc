Function Fragment_Stage_0320_Item_00()
    SetObjectiveDisplayed(300, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_0450_Item_00()
    SetObjectiveFailed(300, True)
    Stop()
EndFunction

Function Fragment_Stage_1000_Item_00()
    Stop()
EndFunction
