Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(50, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction
