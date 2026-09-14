Function MTNMQ_HandleDamagedUplinkAdded(Form akBaseItem, ObjectReference akItemReference)
    If akBaseItem != FS01_MQ_Warn_BrokenUplinkMiscItem || IsCompleted() || IsStageDone(400) || !IsStageDone(300)
        Return
    EndIf

    If akItemReference != None && DamagedUplink.GetReference() != akItemReference
        DamagedUplink.ForceRefTo(akItemReference)
    EndIf
    SetStage(400)
EndFunction

Function MTNMQ_ReconcileProgress()
    Actor playerRef = currentPlayer.GetActorReference()
    If playerRef == None || !IsRunning() || IsCompleted()
        Return
    EndIf

    If IsStageDone(300) && !IsStageDone(400) && FS01_MQ_Warn_BrokenUplinkMiscItem != None && playerRef.GetItemCount(FS01_MQ_Warn_BrokenUplinkMiscItem) > 0
        SetStage(400)
    ElseIf IsStageDone(200) && !IsStageDone(300) && MTNL01_Raiders != None && MTNL01_Raiders.IsCompleted()
        SetStage(300)
    EndIf
EndFunction

Function MTNMQ_PlayMadiganScene()
    If bMadiganScenePlayed || MTN_MQ_Rose_MadiganScene == None
        Return
    EndIf

    bMadiganScenePlayed = True
    If !MTN_MQ_Rose_MadiganScene.IsPlaying()
        MTN_MQ_Rose_MadiganScene.Start()
    EndIf
EndFunction

Event OnQuestInit()
    Parent.OnQuestInit()
    MTNMQ_ReconcileProgress()
EndEvent
