Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(10, True)
    SetStage(1000)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveFailed(10, True)
    SetStage(1000)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(10, False)
    SetStage(1000)
EndFunction

Function Fragment_Stage_1000_Item_00()
    Stop()
EndFunction
