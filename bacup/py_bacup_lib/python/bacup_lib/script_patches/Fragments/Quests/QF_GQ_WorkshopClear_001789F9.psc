Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(200)
EndFunction

Function Fragment_Stage_0500_Item_00()
    If IsObjectiveDisplayed(100) && !IsObjectiveCompleted(100)
        SetObjectiveCompleted(100)
    EndIf
    SetObjectiveCompleted(200)
    Stop()
EndFunction

Function Fragment_Stage_1000_Item_00()
    If IsObjectiveDisplayed(100) && !IsObjectiveCompleted(100)
        SetObjectiveFailed(100)
    EndIf
    If IsObjectiveDisplayed(200) && !IsObjectiveCompleted(200)
        SetObjectiveFailed(200)
    EndIf
    Stop()
EndFunction
