Function Fragment_Stage_0100_Item_00()
EndFunction

Function Fragment_Stage_0110_Item_00()
    If Alias_owningPlayer.GetRef().GetValue(W05_Wayward_PollyStartedIntro) == 0
        W05_DialogueTheWayward_Polly_IntroSceneStart.Start()
        Alias_owningPlayer.GetRef().SetValue(W05_Wayward_PollyStartedIntro, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    Alias_owningPlayer.GetRef().SetValue(W05_Wayward_PollyIntroAttractIndex, 1.0)
EndFunction

Function Fragment_Stage_0300_Item_00()
    Alias_owningPlayer.GetRef().SetValue(W05_Wayward_PlayerCollectedDuchessHolotape, 1.0)
    Alias_DuchessTape.Clear()
EndFunction
