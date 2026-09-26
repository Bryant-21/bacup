Function Fragment_Stage_0090_Item_00()
    EN05_Intro_MiscScript controller = (Self as Quest) as EN05_Intro_MiscScript
    If controller != None
        controller.EN05Intro_AdvanceTraining()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Actor playerRef = Game.GetPlayer()
    If Alias_currentPlayer != None && Alias_currentPlayer.GetActorReference() != None
        playerRef = Alias_currentPlayer.GetActorReference()
    EndIf
    If playerRef != None && EN05_IntroMisc_CompletedValue != None
        playerRef.SetValue(EN05_IntroMisc_CompletedValue, 1.0)
    EndIf
    Stop()
EndFunction
