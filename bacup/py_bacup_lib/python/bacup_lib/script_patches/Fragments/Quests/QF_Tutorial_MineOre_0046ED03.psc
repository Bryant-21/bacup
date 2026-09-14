Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_0020_Item_00()
    If IsStageDone(30) && IsStageDone(40) && !IsStageDone(100)
        SetObjectiveCompleted(10)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
    If IsStageDone(20) && IsStageDone(40) && !IsStageDone(100)
        SetObjectiveCompleted(10)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0040_Item_00()
    If IsStageDone(20) && IsStageDone(30) && !IsStageDone(100)
        SetObjectiveCompleted(10)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    If !IsObjectiveCompleted(10)
        SetObjectiveCompleted(10)
    EndIf
EndFunction
