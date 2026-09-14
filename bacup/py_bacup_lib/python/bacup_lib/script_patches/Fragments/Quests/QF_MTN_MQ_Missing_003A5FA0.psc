Function Fragment_Stage_0050_Item_00()
    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef != None && MTN_MQ_StartedValue != None
        playerRef.SetValue(MTN_MQ_StartedValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True)

    If MTNL01_Raiders != None && MTNL01_Raiders.IsCompleted() && !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(300, True)

    Actor playerRef = Alias_currentPlayer.GetActorReference()
    If playerRef != None && FS01_MQ_Warn_BrokenUplinkMiscItem != None && playerRef.GetItemCount(FS01_MQ_Warn_BrokenUplinkMiscItem) > 0 && !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    MTN_MQ_QuestScript controller = (Self as Quest) as MTN_MQ_QuestScript
    If controller != None
        controller.MTNMQ_PlayMadiganScene()
    ElseIf MTN_MQ_Rose_MadiganScene != None && !MTN_MQ_Rose_MadiganScene.IsPlaying()
        MTN_MQ_Rose_MadiganScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300, True)
    SetObjectiveDisplayed(400, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(400, True)

    Actor playerRef = Alias_currentPlayer.GetActorReference()
    CompleteQuest()

    Bool fsQuestStarted = playerRef != None && FS01_MQ_Warn_QuestActiveKeyword != None && playerRef.HasKeyword(FS01_MQ_Warn_QuestActiveKeyword)
    If !fsQuestStarted && playerRef != None && FS01_Warn_QuestStartKeyword != None
        fsQuestStarted = FS01_Warn_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
    EndIf
    If fsQuestStarted && FS01_MQ_Warn_StartedValue != None
        Alias_currentPlayer.TryToSetValue(FS01_MQ_Warn_StartedValue, 1.0)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveDisplayed(100, False)
    SetObjectiveDisplayed(200, False)
    SetObjectiveDisplayed(300, False)
    SetObjectiveDisplayed(400, False)

    If MTN_MQ_Rose_MadiganScene != None && MTN_MQ_Rose_MadiganScene.IsPlaying()
        MTN_MQ_Rose_MadiganScene.Stop()
    EndIf
    If Alias_PlayerFoundMadigan.GetReference() != None
        Alias_PlayerFoundMadigan.Clear()
    EndIf
    If Alias_DamagedUplink.GetReference() != None
        Alias_DamagedUplink.Clear()
    EndIf
EndFunction
