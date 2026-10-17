; Objective 100 is the kill count that DefaultQuestOnKillManager completes and that B21:ObjectiveTimers expires into stage 900.
Function Fragment_Stage_0000_Item_00()
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0900_Item_00()
    SetObjectiveFailed(100, True)
    Stop()
EndFunction

Function Fragment_Stage_1000_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction
