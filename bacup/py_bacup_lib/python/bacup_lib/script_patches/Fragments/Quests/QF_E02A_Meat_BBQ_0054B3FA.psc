; The progress bar master (stage 150 display, 9000 at full, 9999 at empty) never touches objectives, and B21:QuestTimer ends the event at stage 400.
Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(5, True)
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveCompleted(5, True)
    SetObjectiveDisplayed(10, True)
    SetObjectiveDisplayed(30, True)
    SetObjectiveDisplayed(40, True)
    SetObjectiveDisplayed(50, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    If !IsStageDone(9000)
        SetStage(9999)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9999_Item_00()
    FailAllObjectives()
    Stop()
EndFunction
