Function Fragment_Stage_0000_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(100, True)
EndFunction

; The record has no attacker spawner for "Defend the drop site", so the quest closes here rather than parking on an objective nothing can finish.
Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True)
    SetObjectiveCompleted(200, True)
    SetStage(1000)
EndFunction

Function Fragment_Stage_1000_Item_00()
    CompleteQuest()
EndFunction
