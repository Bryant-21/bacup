Event OnQuestInit()
    InitializeActivePlayer()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == iPlayerRegisteredStage
        RegisterPlayer(False)
    ElseIf auiStageID == iPlayerCompletedExamStage
        CompleteExam(iPlayerCorrectAnswers, False)
    ElseIf auiStageID == iPlayerAcquiredFEVStage
        MarkAcquiredFEV(False)
    ElseIf auiStageID == iPlayerReadWestTekLogStage
        MarkWestTekLogRead(False)
    ElseIf auiStageID == iPlayerStartedOrbitalStage
        MarkOrbitalPlatformStarted(False)
    EndIf
EndEvent

Function InitializeActivePlayer()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && ActivePlayer.GetRef() != playerRef
        ActivePlayer.ForceRefTo(playerRef)
    EndIf
EndFunction

Function RegisterPlayer(Bool abSetStage = True)
    InitializeActivePlayer()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && EN02_PlayerRegisteredValue != None
        playerRef.SetValue(EN02_PlayerRegisteredValue, 1.0)
    EndIf
    If abSetStage && iPlayerRegisteredStage > 0 && !IsStageDone(iPlayerRegisteredStage)
        SetStage(iPlayerRegisteredStage)
    EndIf
EndFunction

Function CompleteExam(Int aiCorrectAnswers = -1, Bool abSetStage = True)
    If aiCorrectAnswers < 0
        EN02_ExamPlayerScript playerScript = Game.GetPlayer() as EN02_ExamPlayerScript
        If playerScript != None
            aiCorrectAnswers = playerScript.iPlayerCorrectAnswers
        Else
            aiCorrectAnswers = 0
        EndIf
    EndIf
    iPlayerCorrectAnswers = aiCorrectAnswers
    If abSetStage && iPlayerCompletedExamStage > 0 && !IsStageDone(iPlayerCompletedExamStage)
        SetStage(iPlayerCompletedExamStage)
    EndIf
EndFunction

Function MarkAcquiredFEV(Bool abSetStage = True)
    If abSetStage && iPlayerAcquiredFEVStage > 0 && !IsStageDone(iPlayerAcquiredFEVStage)
        SetStage(iPlayerAcquiredFEVStage)
    EndIf
EndFunction

Function MarkWestTekLogRead(Bool abSetStage = True)
    If abSetStage && iPlayerReadWestTekLogStage > 0 && !IsStageDone(iPlayerReadWestTekLogStage)
        SetStage(iPlayerReadWestTekLogStage)
    EndIf
EndFunction

Function MarkOrbitalPlatformStarted(Bool abSetStage = True)
    If abSetStage && iPlayerStartedOrbitalStage > 0 && !IsStageDone(iPlayerStartedOrbitalStage)
        SetStage(iPlayerStartedOrbitalStage)
    EndIf
EndFunction
