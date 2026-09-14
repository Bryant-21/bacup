Actor Function PlayerReference()
    Return Alias_Player.GetActorReference()
EndFunction

Function StartSceneIfStopped(Scene sceneToStart)
    If sceneToStart != None && !sceneToStart.IsPlaying()
        sceneToStart.Start()
    EndIf
EndFunction

Function BeginShowMusic()
    If !ShowMusicPlaying && CSDuckMusic != None
        CSDuckMusic.Push(1.0)
        ShowMusicPlaying = True
    EndIf
EndFunction

Function EndShowMusic()
    If ShowMusicPlaying && CSDuckMusic != None
        CSDuckMusic.Remove()
    EndIf
    ShowMusicPlaying = False
EndFunction

Function PlayAudienceReaction(Bool positive)
    Actor player = PlayerReference()
    Scene reactionScene = AC_MQ02_Stage_AudienceReaction_Negative
    Sound reactionSound = QSTACMQ02CrowdNegative
    Idle reactionIdle = IdleBooingStanding
    If positive
        reactionScene = AC_MQ02_Stage_AudienceReaction_Positive
        reactionSound = QSTACMQ02CrowdPositive
        reactionIdle = IdleCheeringStanding
    EndIf
    If player != None && reactionSound != None
        reactionSound.Play(player)
    EndIf
    StartSceneIfStopped(reactionScene)
    Int index = 0
    While index < Alias_Actors_Audience.GetCount()
        Actor audienceMember = Alias_Actors_Audience.GetAt(index) as Actor
        If audienceMember != None
            audienceMember.PlayIdle(reactionIdle)
        EndIf
        index += 1
    EndWhile
EndFunction

Function PrepareKnifeThrowing()
    Actor player = PlayerReference()
    ObjectReference throwMarker = Alias_Marker_ThrowKnifeFrom.GetReference()
    If player == None
        Return
    EndIf
    If throwMarker != None
        player.MoveTo(throwMarker)
    EndIf
    If player.GetItemCount(AC_MQ02_Stage_ThrowingKnife_Weapon) < 1
        player.AddItem(AC_MQ02_Stage_ThrowingKnife_Weapon, 1, True)
    EndIf
    player.EquipItem(AC_MQ02_Stage_ThrowingKnife_Weapon, False, True)
EndFunction

Function RestoreZayde()
    Actor zayde = Alias_Actor_Zayde.GetActorReference()
    If zayde != None
        zayde.Enable()
        zayde.EvaluatePackage()
    EndIf
EndFunction

Function ShutdownLocalShow()
    EndShowMusic()
    If AC_MQ02_Stage_AudienceReaction_Negative != None && AC_MQ02_Stage_AudienceReaction_Negative.IsPlaying()
        AC_MQ02_Stage_AudienceReaction_Negative.Stop()
    EndIf
    If AC_MQ02_Stage_AudienceReaction_Positive != None && AC_MQ02_Stage_AudienceReaction_Positive.IsPlaying()
        AC_MQ02_Stage_AudienceReaction_Positive.Stop()
    EndIf
EndFunction

Event OnQuestInit()
    QuestLoc = Alias_Loc_Pier.GetLocation()
    RestoreZayde()
EndEvent

Event OnQuestShutdown()
    ShutdownLocalShow()
EndEvent
