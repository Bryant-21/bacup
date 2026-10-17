Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_9999_Item_00()
    CompleteAllObjectives()
    Utility.Wait(1.0)
    Stop()
EndFunction
