; Options A01/A02 and B01/B02 are alternatives, so stages 11/12 and 21/22 each replace the craft objective with one drop objective.
Function CompleteIfDisplayed(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective)
        SetObjectiveCompleted(aiObjective, True)
    EndIf
EndFunction

Function CompleteWhenBothDropped()
    If IsStageDone(30) && IsStageDone(40) && !IsStageDone(50)
        SetStage(50)
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0011_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(11, True)
EndFunction

Function Fragment_Stage_0012_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(12, True)
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0021_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(21, True)
EndFunction

Function Fragment_Stage_0022_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(22, True)
EndFunction

Function Fragment_Stage_0030_Item_00()
    CompleteIfDisplayed(10)
    CompleteIfDisplayed(11)
    CompleteIfDisplayed(12)
    CompleteWhenBothDropped()
EndFunction

Function Fragment_Stage_0040_Item_00()
    CompleteIfDisplayed(20)
    CompleteIfDisplayed(21)
    CompleteIfDisplayed(22)
    CompleteWhenBothDropped()
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(50, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    CompleteAllObjectives()
    If MTR01_Flame_0100_WrapUp != None
        MTR01_Flame_0100_WrapUp.Start()
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    Stop()
EndFunction
