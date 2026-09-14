Function Fragment_Stage_0000_Item_00()
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10, True)
    Actor playerRef = Alias_MTR07_EarthPlayer.GetActorReference()
    MiscObject ignitionCore = Game.GetFormFromFile(0x0015C3, "SeventySix.esm") as MiscObject
    If playerRef != None && ignitionCore != None
        Int coreCount = playerRef.GetItemCount(ignitionCore)
        If coreCount < 4
            playerRef.AddItem(ignitionCore, 4 - coreCount, True)
        EndIf
        If !IsStageDone(20)
            SetStage(20)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True)
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveDisplayed(20, True)
    TryAdvanceAfterCoreInstalled()
EndFunction

Function Fragment_Stage_0040_Item_00()
    SetObjectiveDisplayed(20, True)
    TryAdvanceAfterCoreInstalled()
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(20, True)
    TryAdvanceAfterCoreInstalled()
EndFunction

Function Fragment_Stage_0060_Item_00()
    SetObjectiveDisplayed(20, True)
    TryAdvanceAfterCoreInstalled()
EndFunction

Function Fragment_Stage_0070_Item_00()
    SetObjectiveCompleted(20, True)
    SetObjectiveDisplayed(30, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(30, True)
    SetObjectiveDisplayed(40, True)
EndFunction

Function Fragment_Stage_0220_Item_00()
    SetObjectiveDisplayed(40, True)
EndFunction

Function Fragment_Stage_0255_Item_00()
    SetObjectiveCompleted(40, True)
EndFunction
Function TryAdvanceAfterCoreInstalled()
    If IsStageDone(30) && IsStageDone(40) && IsStageDone(50) && IsStageDone(60) && !IsStageDone(70)
        SetStage(70)
    EndIf
EndFunction
