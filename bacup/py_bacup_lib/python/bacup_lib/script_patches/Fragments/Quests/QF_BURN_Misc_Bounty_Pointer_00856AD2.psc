; Stage flow comes from the alias scripts: entering Highway Town or its trigger sets 150,
; the saloon trigger sets 200, and activating the Ghoul or Billie sets 300.
Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0150_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(30, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(30, True)
    SetStage(9000)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction
