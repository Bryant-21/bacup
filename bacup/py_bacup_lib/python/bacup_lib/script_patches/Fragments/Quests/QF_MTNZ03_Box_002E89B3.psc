; Stages 200/300 are the Enclave and DIA deliveries; MTNZ03_DropOffScript is meant to follow with stage 400, the quest end.
Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    If !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(100, True)
    If !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction
