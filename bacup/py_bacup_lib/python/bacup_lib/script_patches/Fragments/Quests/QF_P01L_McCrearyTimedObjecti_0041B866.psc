; The alias scripts advance 200 (intercom) -> 300 (found the speaker) -> 400 (picked up the collar) -> 500 (equipped it) -> 600 (read the note) -> 9000.
Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True)
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(30, True)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(30, True)
    SetObjectiveDisplayed(40, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(40, True)
    SetObjectiveDisplayed(50, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(50, True)
    SetObjectiveDisplayed(60, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteAllObjectives()
    Stop()
EndFunction

Function Fragment_Stage_9990_Item_00()
    FailAllObjectives()
    Stop()
EndFunction
