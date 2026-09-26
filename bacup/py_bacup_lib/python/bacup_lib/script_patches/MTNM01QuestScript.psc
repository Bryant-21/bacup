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
    MTNM01_RegisterCannibalEvents()
    If GetCurrentStageID() < 50
        SetStage(50)
    EndIf
EndEvent

Function MTNM01_RegisterCannibalEvents()
    RegisterForRemoteEvent(Game.GetFormFromFile(0x0004B259, "Fallout4.esm") as Perk, "OnEntryRun")
    RegisterForRemoteEvent(Game.GetFormFromFile(0x001D1A62, "Fallout4.esm") as Perk, "OnEntryRun")
    RegisterForRemoteEvent(Game.GetFormFromFile(0x001D1A63, "Fallout4.esm") as Perk, "OnEntryRun")
    RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
EndFunction

Event Perk.OnEntryRun(Perk akSender, Int auiEntryID, ObjectReference akTarget, Actor akOwner)
    If auiEntryID != 0 || akOwner != Game.GetPlayer() || !IsRunning() \
        || !IsStageDone(CannibalStage) || IsStageDone(CannibalSuccessStage) \
        || IsStageDone(CannibalWalkAwayStage) || IsStageDone(DoneStage)
        Return
    EndIf
    If akSender != Game.GetFormFromFile(0x0004B259, "Fallout4.esm") \
        && akSender != Game.GetFormFromFile(0x001D1A62, "Fallout4.esm") \
        && akSender != Game.GetFormFromFile(0x001D1A63, "Fallout4.esm")
        Return
    EndIf
    Actor corpse = akTarget as Actor
    If corpse == None || !corpse.IsDead()
        Return
    EndIf
    Race corpseRace = corpse.GetRace()
    If corpseRace != Game.GetFormFromFile(0x0006B4EC, "Fallout4.esm") \
        && corpseRace != Game.GetFormFromFile(0x000A96BF, "Fallout4.esm")
        Return
    EndIf
    If GhoulCorpse != None
        GhoulCorpse.ForceRefTo(corpse)
    EndIf
    SetStage(CannibalSuccessStage)
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    MTNM01_RegisterCannibalEvents()
    If IsRunning() && IsStageDone(CannibalStage) && !IsStageDone(CannibalSuccessStage) \
        && !IsStageDone(CannibalWalkAwayStage) && !IsStageDone(DoneStage)
        StartTimer(BaitTimerLength, CannibalWalkAwayStage)
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == KillDeathClawStage
        StartTimer(BaitTimerLength, FledDeathclawStage)
    ElseIf auiStageID == 660 || auiStageID == FledDeathclawStage
        CancelTimer(FledDeathclawStage)
    ElseIf auiStageID == CannibalStage
        MTNM01_RegisterCannibalEvents()
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
    UnregisterForAllRemoteEvents()
    CancelTimer(FledDeathclawStage)
    CancelTimer(CannibalWalkAwayStage)
EndEvent
