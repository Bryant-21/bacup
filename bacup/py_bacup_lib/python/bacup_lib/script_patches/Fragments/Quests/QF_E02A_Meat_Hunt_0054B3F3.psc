; Objectives 7/15/25/35 are "incoming" timer objectives (B21:ObjectiveTimers expires them into the next wave's spawn stage); 10/20/30/40 are the kill objectives.
Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(5, True)
EndFunction

Function Fragment_Stage_0125_Item_00()
    If !IsStageDone(150) && !IsStageDone(9000)
        SetStage(9999)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveCompleted(5, True)
    SetObjectiveDisplayed(7, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(7, True)
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(15, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(15, True)
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0350_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(25, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(25, True)
    SetObjectiveDisplayed(30, True)
EndFunction

Function Fragment_Stage_0450_Item_00()
    SetObjectiveCompleted(30, True)
    SetObjectiveDisplayed(35, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(35, True)
    SetObjectiveDisplayed(40, True)
EndFunction

; Nothing else in the record ends the event after the final wave, so it closes here.
Function Fragment_Stage_0550_Item_00()
    SetObjectiveCompleted(40, True)
    If !IsStageDone(9000)
        SetStage(9000)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0650_Item_00()
    SetObjectiveCompleted(200, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9999_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

; Each round's "A" group is the only one whose EncounterWaves row sets an end stage (201/301/401), so its death closes the round.
Function Fragment_Stage_0201_Item_00()
    If !IsStageDone(250)
        SetStage(250)
    EndIf
EndFunction

Function Fragment_Stage_0301_Item_00()
    If !IsStageDone(350)
        SetStage(350)
    EndIf
EndFunction

Function Fragment_Stage_0401_Item_00()
    If !IsStageDone(450)
        SetStage(450)
    EndIf
EndFunction

; The boss collection (501) is the last wave group with a death stage, so its death ends the final wave.
Function Fragment_Stage_0501_Item_00()
    If !IsStageDone(550)
        SetStage(550)
    EndIf
EndFunction
