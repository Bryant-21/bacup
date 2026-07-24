Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(100)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(100)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveDisplayed(500)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(500)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveCompleted(500)
EndFunction
