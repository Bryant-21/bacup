Function Fragment_Stage_0010_Item_00()
    SetObjectiveCompleted(5, True)
    SetObjectiveDisplayed(10, True)
EndFunction

; Rewards are handed out on this stage; stage 110 is the separate shutdown.
Function Fragment_Stage_0100_Item_00()
    CompleteAllObjectives()
EndFunction

Function Fragment_Stage_0110_Item_00()
    Stop()
EndFunction
