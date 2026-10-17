; Objective 10 and 20 carry FO76 objective timers (B21:ObjectiveTimers expires them into 9991 and 9992).
Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10, True)
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveDisplayed(15, True)
EndFunction

Function Fragment_Stage_0275_Item_00()
    SetObjectiveCompleted(15, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(40, True)
EndFunction

; Objective 35 is the progress-bar objective DefaultAliasProgressBarScript drives on the door terminal.
Function Fragment_Stage_0450_Item_00()
    SetObjectiveCompleted(40, True)
    SetObjectiveDisplayed(30, True)
    SetObjectiveDisplayed(35, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9990_Item_00()
    SetObjectiveFailed(30, True)
    SetObjectiveFailed(35, True)
    Stop()
EndFunction

Function Fragment_Stage_9991_Item_00()
    SetObjectiveFailed(10, True)
    Stop()
EndFunction

Function Fragment_Stage_9992_Item_00()
    SetObjectiveFailed(20, True)
    Stop()
EndFunction

Function Fragment_Stage_9993_Item_00()
    FailAllObjectives()
    Stop()
EndFunction
