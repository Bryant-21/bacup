Event OnQuestInit()
    Actor playerRef = InitializePlayer()
    If playerRef != None && EN01_Misc_CheckpointValue != None
        iInitialCheckpointValue = playerRef.GetValue(EN01_Misc_CheckpointValue) as Int
    EndIf
    If EN01_MQ_Bunker != None
        RegisterForRemoteEvent(EN01_MQ_Bunker, "OnStageSet")
    EndIf
    RestoreCheckpoint()
    ReconcileBunkerProgress()
    RegisterMiscProgressEvents()
EndEvent

; Stage 100 ("entered the trigger" at Blackwell's bunker) has no record-side
; setter; it is what sends the Bunker Buster start event.
Function RegisterMiscProgressEvents()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnLocationChange")
    EndIf
EndFunction

Event Actor.OnLocationChange(Actor akSender, Location akOldLoc, Location akNewLoc)
    If akSender != Game.GetPlayer() || !IsRunning() || IsStageDone(iShutdownStage) || !IsStageDone(iAcquiredQuinnsNotes)
        Return
    EndIf
    LocationAlias bunkerAlias = GetAlias(5) as LocationAlias
    If bunkerAlias == None || akNewLoc == None || akNewLoc != bunkerAlias.GetLocation()
        Return
    EndIf
    If !IsStageDone(iReadQuinnsNotesStage)
        SetStage(iReadQuinnsNotesStage)
    EndIf
    SetStage(iShutdownStage)
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    Actor playerRef = InitializePlayer()
    If auiStageID == iReadQuinnsNotesStage
        If playerRef != None && EN01_Misc_ReadInterviewNotes != None
            playerRef.SetValue(EN01_Misc_ReadInterviewNotes, 1.0)
        EndIf
    ElseIf auiStageID == iShutdownStage
        StartBunkerQuest()
    EndIf
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If akSender == EN01_MQ_Bunker && auiStageID == iEN01BunkerStartUpStage
        ReconcileBunkerProgress()
    EndIf
EndEvent

Actor Function InitializePlayer()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && currentPlayer != None && currentPlayer.GetRef() != playerRef
        currentPlayer.ForceRefTo(playerRef)
    EndIf
    Return playerRef
EndFunction

Function RestoreCheckpoint()
    If bCheckpointRestored
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef == None
        Return
    EndIf
    Int restoredValue = iInitialCheckpointValue
    If EN01_Misc_ReadInterviewNotes != None && playerRef.GetValue(EN01_Misc_ReadInterviewNotes) >= 1.0 && restoredValue < ReadQuinnsNoteAV
        restoredValue = ReadQuinnsNoteAV
    EndIf
    If restoredValue < AcquiredQuinnsNoteAV
        Return
    EndIf
    bCheckpointRestored = True
    If EN01_QuinnCarterBlackwellNotes != None && playerRef.GetItemCount(EN01_QuinnCarterBlackwellNotes) <= 0
        playerRef.AddItem(EN01_QuinnCarterBlackwellNotes, 1, True)
    EndIf
    If !IsStageDone(iAcquiredQuinnsNotes)
        SetStage(iAcquiredQuinnsNotes)
    EndIf
    If restoredValue >= ReadQuinnsNoteAV && !IsStageDone(iReadQuinnsNotesStage)
        SetStage(iReadQuinnsNotesStage)
    EndIf
EndFunction

Function StartBunkerQuest()
    Actor playerRef = Game.GetPlayer()
    If EN01_QuestStartKeyword != None && playerRef != None && EN01_MQ_Bunker != None && !EN01_MQ_Bunker.IsRunning() && !EN01_MQ_Bunker.IsCompleted()
        EN01_QuestStartKeyword.SendStoryEvent(None, playerRef, playerRef)
    EndIf
EndFunction

Function ReconcileBunkerProgress()
    If EN01_MQ_Bunker == None || !EN01_MQ_Bunker.IsRunning()
        Return
    EndIf
    If EN01_MQ_Bunker.IsStageDone(iEN01BunkerStartUpStage) && !EN01_MQ_Bunker.IsStageDone(iEN01ProgressStage)
        EN01_MQ_Bunker.SetStage(iEN01ProgressStage)
    EndIf
EndFunction
