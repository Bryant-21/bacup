Function Fragment_Stage_0100_Item_00()
    W05_MQ_TheWayward_QuestScript controller = (Self as Quest) as W05_MQ_TheWayward_QuestScript
    If controller != None
        controller.RefreshPollyAlias()
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    TryStartPollyIntro()
EndFunction

Function TryStartPollyIntro()
    If !IsRunning() || Alias_owningPlayer == None || W05_Wayward_PollyStartedIntro == None
        Return
    EndIf
    Actor playerRef = Alias_owningPlayer.GetActorReference()
    If playerRef == None || playerRef != Game.GetPlayer() || playerRef.GetValue(W05_Wayward_PollyStartedIntro) != 0.0
        Return
    EndIf
    W05_MQ_TheWayward_QuestScript controller = (Self as Quest) as W05_MQ_TheWayward_QuestScript
    If controller == None || controller.RefreshPollyAlias() == None
        Return
    EndIf
    If W05_DialogueTheWayward_Polly_IntroSceneStart != None && !W05_DialogueTheWayward_Polly_IntroSceneStart.IsPlaying()
        W05_DialogueTheWayward_Polly_IntroSceneStart.Start()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && W05_Wayward_PollyIntroAttractIndex != None
        playerRef.SetValue(W05_Wayward_PollyIntroAttractIndex, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && W05_Wayward_PlayerCollectedDuchessHolotape != None
        playerRef.SetValue(W05_Wayward_PlayerCollectedDuchessHolotape, 1.0)
    EndIf
    If Alias_DuchessTape != None
        Alias_DuchessTape.Clear()
    EndIf
EndFunction
