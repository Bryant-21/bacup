Function Fragment_Stage_0000_Item_00()
    If !IsStageDone(1000)
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveDisplayed(1000)
EndFunction

Function Fragment_Stage_1000_Item_01()
    SetObjectiveDisplayed(1000)
EndFunction

Function Fragment_Stage_1000_Item_02()
    SetObjectiveDisplayed(1000)
EndFunction

Function Fragment_Stage_5000_Item_00()
    SetObjectiveCompleted(1000)
    SetObjectiveDisplayed(5000)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    CompleteQuest()
    Stop()
EndFunction

Function Fragment_Stage_9500_Item_00()
    FailAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9999_Item_00()
    SetObjectiveDisplayed(1000, False)
    SetObjectiveDisplayed(5000, False)
EndFunction
