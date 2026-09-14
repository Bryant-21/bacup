Actor Function MTR06_GetPlayer()
    Actor playerRef = ActivePlayer.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
        If playerRef != None && ActivePlayer.GetReference() == None
            ActivePlayer.ForceRefTo(playerRef)
        EndIf
    EndIf
    Return playerRef
EndFunction

Function MTR06_ReconcileCheckpoint()
    If bStartupOnce || IsCompleted()
        Return
    EndIf

    Actor playerRef = MTR06_GetPlayer()
    If playerRef == None || MTR06_CheckpointValue == None
        Return
    EndIf

    bStartupOnce = True
    fStartupCheckpointVal = playerRef.GetValue(MTR06_CheckpointValue)
    bPECKComplete = fStartupCheckpointVal >= iCompletedPEAV
    bFinalComplete = fStartupCheckpointVal >= iCompletedFinalAV

    Int targetStage = -1
    If fStartupCheckpointVal >= iCompletedFinalAV
        targetStage = iFinalWrappedUpStage
    ElseIf fStartupCheckpointVal >= iCompletedPEAV
        targetStage = iPhysicalWrapUpStage
    ElseIf fStartupCheckpointVal >= iCompletedKnowledgeAV
        targetStage = iKnowledgeExamStage
    ElseIf fStartupCheckpointVal >= iCompletedMiscAV
        targetStage = iMiscCompletedStage
    EndIf

    If targetStage >= 0 && GetStage() < targetStage && !IsStageDone(targetStage)
        SetStage(targetStage)
    EndIf
EndFunction

Function MTR06_HandlePhysicalExamComplete()
    Actor playerRef = MTR06_GetPlayer()
    If playerRef == None || MTR06_PhysExamCompleted == None || !IsRunning() || IsCompleted()
        Return
    EndIf

    If playerRef.GetValue(MTR06_PhysExamCompleted) >= 1.0 && GetStage() < iPhysicalCompleteStage && !IsStageDone(iPhysicalCompleteStage)
        SetStage(iPhysicalCompleteStage)
    EndIf
EndFunction

Event OnQuestInit()
    MTR06_ReconcileCheckpoint()
EndEvent
