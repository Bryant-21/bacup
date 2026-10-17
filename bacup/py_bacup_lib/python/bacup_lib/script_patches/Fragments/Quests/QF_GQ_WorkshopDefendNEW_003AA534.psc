; Objectives 100 (workshop) and 150 (camp) are alternative targets of the same attack, not a sequence.
Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveDisplayed(150, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction
