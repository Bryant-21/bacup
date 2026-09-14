Actor Function MTNM01_GetPlayer()
    If currentPlayer
        Actor playerRef = currentPlayer.GetActorReference()
        If playerRef
            Return playerRef
        EndIf
    EndIf
    Return Game.GetPlayer()
EndFunction

Event OnQuestInit()
    If GetCurrentStageID() < 50
        SetStage(50)
    ElseIf IsStageDone(250) && !IsStageDone(300)
        SetStage(300)
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 250 && !IsStageDone(UseChemDartStage)
        SetStage(UseChemDartStage)
    ElseIf auiStageID == KillDeathClawStage
        StartTimer(BaitTimerLength, FledDeathclawStage)
    ElseIf auiStageID == 660 || auiStageID == FledDeathclawStage
        CancelTimer(FledDeathclawStage)
    ElseIf auiStageID == CannibalStage
        StartTimer(BaitTimerLength, CannibalWalkAwayStage)
    ElseIf auiStageID == CannibalSuccessStage || auiStageID == CannibalWalkAwayStage || auiStageID == DoneStage
        CancelTimer(CannibalWalkAwayStage)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == FledDeathclawStage
        If !IsRunning() || IsStageDone(660) || IsStageDone(FledDeathclawStage)
            Return
        EndIf

        Actor playerRef = MTNM01_GetPlayer()
        Actor deathclawRef
        If Deathclaw
            deathclawRef = Deathclaw.GetActorReference()
        EndIf
        If playerRef && deathclawRef && !deathclawRef.IsDead()
            If playerRef.GetDistance(deathclawRef) > fSpawnDistance
                SetStage(FledDeathclawStage)
            Else
                StartTimer(BaitTimerLength, FledDeathclawStage)
            EndIf
        EndIf
    ElseIf aiTimerID == CannibalWalkAwayStage
        If !IsRunning() || IsStageDone(CannibalSuccessStage) || IsStageDone(CannibalWalkAwayStage) || IsStageDone(DoneStage)
            Return
        EndIf

        Actor playerRef = MTNM01_GetPlayer()
        Actor ghoulRef
        If GhoulCorpse
            ghoulRef = GhoulCorpse.GetActorReference()
        EndIf
        If playerRef && ghoulRef
            If playerRef.GetDistance(ghoulRef) > fSpawnDistance
                SetStage(CannibalWalkAwayStage)
            Else
                StartTimer(BaitTimerLength, CannibalWalkAwayStage)
            EndIf
        EndIf
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(FledDeathclawStage)
    CancelTimer(CannibalWalkAwayStage)
EndEvent
