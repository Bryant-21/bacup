Function Fragment_Stage_0020_Item_00()
    SetObjectiveDisplayed(20)
EndFunction

Function Fragment_Stage_0021_Item_00()
    SetObjectiveCompleted(20)
    SetObjectiveDisplayed(30)
EndFunction

Function Fragment_Stage_0031_Item_00()
    SetObjectiveCompleted(30)
EndFunction

Function Fragment_Stage_0040_Item_00()
    SetObjectiveDisplayed(40)
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveCompleted(40)
    SetObjectiveDisplayed(50)
EndFunction

Function Fragment_Stage_0060_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(60)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(60)
    CompleteAllObjectives()
    Stop()
EndFunction
