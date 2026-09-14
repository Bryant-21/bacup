Event OnQuestInit()
    InitializePlayer()
    If EN01_Bunker != None && !EN01_Bunker.IsRunning()
        EN01_Bunker.Start()
    EndIf
    If iStartUpStage > 0 && !IsStageDone(iStartUpStage)
        SetStage(iStartUpStage)
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    Actor playerRef = InitializePlayer()
    If auiStageID == iPlayerAccessedBunkerStage
        If playerRef != None && EN01_CheckpointValue != None
            playerRef.SetValue(EN01_CheckpointValue, AccessedSamsBunkerAV as Float)
        EndIf
    ElseIf auiStageID == 83 || auiStageID == 86 || auiStageID == 89
        CheckPowerReset()
    ElseIf auiStageID == 112
        SetObjectiveDisplayed(122, True)
    ElseIf auiStageID == 115 || auiStageID == 117 || auiStageID == 119
        RecordCodeClue(auiStageID)
    ElseIf auiStageID == iDirectoryFoundStage
        DisplayKnownCodes()
    EndIf
EndEvent

Actor Function InitializePlayer()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && currentPlayer.GetRef() != playerRef
        currentPlayer.ForceRefTo(playerRef)
    EndIf
    Return playerRef
EndFunction

Function HandleImportantNoteRead(Book akBook)
    If akBook == None || !IsRunning()
        Return
    EndIf

    Int index = 0
    While NoteData != None && index < NoteData.Length
        NoteDatum clue = NoteData[index]
        If clue.TargetBook == akBook
            If !IsStageDone(clue.iStageToSetOnRead)
                SetStage(clue.iStageToSetOnRead)
            EndIf
            RecordCodeClue(clue.iStageToSetOnRead)
            Return
        EndIf
        index += 1
    EndWhile
EndFunction

Function CheckPowerReset()
    If IsStageDone(83) && IsStageDone(86) && IsStageDone(89) && !IsStageDone(90)
        SetStage(90)
    EndIf
EndFunction

Function RecordCodeClue(Int aiStage)
    Actor playerRef = InitializePlayer()
    Int index = 0
    While NoteData != None && index < NoteData.Length
        NoteDatum clue = NoteData[index]
        If clue.iStageToSetOnRead == aiStage
            If playerRef != None && clue.myActorValue != None
                playerRef.SetValue(clue.myActorValue, 1.0)
            EndIf
            If clue.iObjectiveIndex > 0
                SetObjectiveDisplayed(clue.iObjectiveIndex, True)
            EndIf
            Return
        EndIf
        index += 1
    EndWhile
EndFunction

Function DisplayKnownCodes()
    SetObjectiveDisplayed(iSearchForCluesObjIndex, True)
    If IsStageDone(112)
        SetObjectiveDisplayed(122, True)
    EndIf
    Int index = 0
    While NoteData != None && index < NoteData.Length
        If IsStageDone(NoteData[index].iStageToSetOnRead) && NoteData[index].iObjectiveIndex > 0
            SetObjectiveDisplayed(NoteData[index].iObjectiveIndex, True)
        EndIf
        index += 1
    EndWhile
EndFunction
