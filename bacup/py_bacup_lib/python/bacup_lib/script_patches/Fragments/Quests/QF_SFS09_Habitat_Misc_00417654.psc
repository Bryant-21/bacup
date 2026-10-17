Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction
