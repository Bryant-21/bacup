Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveDisplayed(100, False)
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_1000_Item_00()
    FailAllObjectives()
    Stop()
EndFunction
