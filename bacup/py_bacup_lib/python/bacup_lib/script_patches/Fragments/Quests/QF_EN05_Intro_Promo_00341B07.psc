Function Fragment_Stage_0010_Item_00()
    EN05_Intro_MiscScript controller = (Self as Quest) as EN05_Intro_MiscScript
    If controller != None
        controller.EN05Intro_FillPlayer()
    EndIf
EndFunction

Function Fragment_Stage_0090_Item_00()
    EN05_Intro_MiscScript controller = (Self as Quest) as EN05_Intro_MiscScript
    If controller != None
        controller.EN05Intro_AdvanceTraining()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Actor player = Game.GetPlayer()
    If Alias_currentPlayer != None && Alias_currentPlayer.GetActorReference() != None
        player = Alias_currentPlayer.GetActorReference()
    EndIf
    If player != None && EN05_OfficerMisc_CompletedValue != None
        player.SetValue(EN05_OfficerMisc_CompletedValue, 1.0)
    EndIf
    Stop()
EndFunction
