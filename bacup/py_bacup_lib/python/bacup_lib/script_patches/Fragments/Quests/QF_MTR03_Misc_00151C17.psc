Function Fragment_Stage_0010_Item_00()
    SetObjectiveDisplayed(10, True)
    If MTR03_Misc_0010_AttractScene && !MTR03_Misc_0010_AttractScene.IsPlaying()
        MTR03_Misc_0010_AttractScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    CompleteAllObjectives()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(MTR03_MiscComplete, 1.0)
    EndIf
    Stop()
EndFunction

Function Fragment_Stage_0015_Item_00()
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(15, True)
EndFunction
