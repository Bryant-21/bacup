; Stage notes: 100 entered the terminal (needs recipe), 200 has the recipe, 300 crafted the incense, 400 used it in the brazier.
Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(300, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(300, True)
    SetObjectiveDisplayed(400, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction
