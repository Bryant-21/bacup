Function Fragment_Stage_0010_Item_00()
    Actor player = Game.GetPlayer()
    If Alias_CurrentPlayer != None && Alias_CurrentPlayer.GetActorReference() != None
        player = Alias_CurrentPlayer.GetActorReference()
    EndIf
    If player != None && EN07_Death_PlayerHitModusSiloTutorial != None
        player.SetValue(EN07_Death_PlayerHitModusSiloTutorial, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0020_Item_00()
    Actor player = Game.GetPlayer()
    If Alias_CurrentPlayer != None && Alias_CurrentPlayer.GetActorReference() != None
        player = Alias_CurrentPlayer.GetActorReference()
    EndIf
    If player != None && EN07_Death_PlayerDidSiloTutorial != None
        player.SetValue(EN07_Death_PlayerDidSiloTutorial, 1.0)
    EndIf
    EN07_IntroMiscScript mainQuest = Game.GetFormFromFile(0x002D0F6B, "SeventySix.esm") as EN07_IntroMiscScript
    If mainQuest != None
        mainQuest.CreditLocalTutorialQuest(Self)
    EndIf
    EN07_PosterTutorialScript tutorial = (Self as Quest) as EN07_PosterTutorialScript
    If tutorial != None
        tutorial.EN07Tutorial_ResetLights()
    EndIf
    Stop()
EndFunction
