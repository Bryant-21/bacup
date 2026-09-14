Function Fragment_Stage_0090_Item_00()
    If EN05_Intro_Misc_Scene && !EN05_Intro_Misc_Scene.IsPlaying()
        EN05_Intro_Misc_Scene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef
        playerRef.SetValue(EN05_IntroMisc_CompletedValue, 1.0)
    EndIf
    Stop()
EndFunction
