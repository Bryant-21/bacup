Event OnQuestInit()
    InitializePlayer()
    If iStartingStage <= 0
        iStartingStage = 5
    EndIf
    If iDeconCompleteStage <= 0
        iDeconCompleteStage = 37
    EndIf
    If iExamCompleteStage <= 0
        iExamCompleteStage = 170
    EndIf
    If iModuleInsertedStage <= 0
        iModuleInsertedStage = 315
    EndIf
    If iModuleCompleteStage <= 0
        iModuleCompleteStage = 320
    EndIf
    If iTriggerDropStage <= 0
        iTriggerDropStage = 340
    EndIf
    If !IsStageDone(iStartingStage)
        SetStage(iStartingStage)
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    UpdateCheckpoint(auiStageID)
    If auiStageID == 35
        StartDecontamination()
    ElseIf auiStageID == 140
        ResetExam()
    ElseIf auiStageID == iExamCompleteStage
        CompleteExam()
    ElseIf auiStageID == iModuleInsertedStage
        BeginModuleSequence()
    ElseIf auiStageID == iModuleCompleteStage
        StartTimer(iModuleSceneTimerLength as Float, iModuleSceneTimeID)
    ElseIf auiStageID == 340
        BeginOrbitalDrop()
    ElseIf auiStageID == 350
        iOrbitalDropFailSafeCount = 0
        StartTimer(1.0, iBroadcastFailsafeID)
    EndIf
EndEvent

Function EN02_HandleModuleStep(Int aiStage)
    If !IsRunning() || IsCompleted()
        Return
    EndIf
    If aiStage == 317
        EnableAliasRef(PowerUpMarker)
    ElseIf aiStage == 318
        DisableAliasRef(ActiveSparks)
    Else
        Return
    EndIf
    StartTimer(0.5, iModuleSceneTimeID)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 3
        FinishDecontamination()
    ElseIf aiTimerID == iModuleSceneTimeID
        AdvanceModuleSequence()
    ElseIf aiTimerID == iOrbitalDropTimerID
        SpawnOrbitalDrop()
    ElseIf aiTimerID == iBroadcastFailsafeID
        CheckOrbitalRewards()
    EndIf
EndEvent

Function InitializePlayer()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && currentPlayer.GetRef() != playerRef
        currentPlayer.ForceRefTo(playerRef)
    EndIf
    ENB_BunkerMasterScript bunker = ENB_BunkerMasterQuest as ENB_BunkerMasterScript
    If bunker != None
        bunker.InitializeDeconController()
    EndIf
EndFunction

Function StartDecontamination()
    InitializePlayer()
    Float delay = iDeconTimerLength as Float
    If delay <= 0.0
        delay = 5.0
    EndIf
    StartTimer(delay, 3)
EndFunction

Function FinishDecontamination()
    If iDeconCompleteStage > 0 && !IsStageDone(iDeconCompleteStage)
        SetStage(iDeconCompleteStage)
    EndIf
    ForcePlayerIntoAlias(PlayerCanBypassArches)
    SetCollectionActivation(DeconArches, False)
    If !IsStageDone(40)
        SetStage(40)
    EndIf
EndFunction

Function ResetExam()
    iCorrectAnswers = 0
    iPerceptionPuzzlesFound = 0
    ResetStoredExam("EN02")
    Actor playerRef = Game.GetPlayer()
    EN02_ExamPlayerScript playerScript = playerRef as EN02_ExamPlayerScript
    If playerScript != None
        playerScript.ResetAnswers()
    EndIf
    If playerRef != None && EN02_ExamScoreValue != None
        playerRef.SetValue(EN02_ExamScoreValue, 0.0)
    EndIf
    SetCollectionActivation(ExamTerminals, False)
EndFunction

Function CompleteExam()
    If !IsRunning() || IsCompleted() || IsStageDone(170)
        Return
    EndIf
    InitializePlayer()
    Actor playerRef = Game.GetPlayer()
    iCorrectAnswers = GetStoredExamScore("EN02")
    If playerRef != None
        If EN02_ExamScoreValue != None
            playerRef.SetValue(EN02_ExamScoreValue, iCorrectAnswers as Float)
        EndIf
        If EN02_PlayerCompletedExamValue != None
            playerRef.SetValue(EN02_PlayerCompletedExamValue, 1.0)
        EndIf
        ForcePlayerIntoAlias(PlayerCompletedExam)
        Float successThreshold = 5.0
        If EN02_ExamSuccessThreshold != None && EN02_ExamSuccessThreshold.GetValue() > 0.0
            successThreshold = EN02_ExamSuccessThreshold.GetValue()
        EndIf
        If iCorrectAnswers as Float >= successThreshold
            ForcePlayerIntoAlias(PlayerAcedExam)
        ElseIf PlayerAcedExam != None
            PlayerAcedExam.Clear()
        EndIf
    EndIf
    If iExamCompleteStage > 0 && !IsStageDone(iExamCompleteStage)
        SetStage(iExamCompleteStage)
    EndIf
EndFunction

Function ResetStoredExam(String asChannel)
    If B21ExamAnswers == None
        B21ExamAnswers = new EN02_ExamPlayerScript:TerminalDatum[0]
        Return
    EndIf
    Int index = 0
    While index < B21ExamAnswers.Length
        EN02_ExamQuestionScript question = B21ExamAnswers[index].TargetTerminal as EN02_ExamQuestionScript
        If question != None && question.DejaChannel == asChannel
            B21ExamAnswers.Remove(index)
        Else
            index += 1
        EndIf
    EndWhile
EndFunction

Function RecordStoredExamAnswer(Terminal akQuestionTerminal, Int aiResponseValue)
    If akQuestionTerminal == None
        Return
    EndIf
    If B21ExamAnswers == None
        B21ExamAnswers = new EN02_ExamPlayerScript:TerminalDatum[0]
    EndIf
    Int index = 0
    While index < B21ExamAnswers.Length
        If B21ExamAnswers[index].TargetTerminal == akQuestionTerminal
            EN02_ExamPlayerScript:TerminalDatum answer = B21ExamAnswers[index]
            answer.iResponseValue = aiResponseValue
            B21ExamAnswers[index] = answer
            Return
        EndIf
        index += 1
    EndWhile
    EN02_ExamPlayerScript:TerminalDatum newAnswer = new EN02_ExamPlayerScript:TerminalDatum
    newAnswer.TargetTerminal = akQuestionTerminal
    newAnswer.iResponseValue = aiResponseValue
    B21ExamAnswers.Add(newAnswer)
EndFunction

Int Function GetStoredExamScore(String asChannel)
    Int score = 0
    If B21ExamAnswers != None
        Int index = 0
        While index < B21ExamAnswers.Length
            EN02_ExamPlayerScript:TerminalDatum answer = B21ExamAnswers[index]
            EN02_ExamQuestionScript question = answer.TargetTerminal as EN02_ExamQuestionScript
            If question != None && question.DejaChannel == asChannel && answer.iResponseValue > 0
                score += answer.iResponseValue
            EndIf
            index += 1
        EndWhile
    EndIf
    Return score
EndFunction

Function BeginModuleSequence()
    InitializePlayer()
    If EN02_Module_QuestStartKeyword != None
        EN02_Module_QuestStartKeyword.SendStoryEvent(None, Game.GetPlayer(), Game.GetPlayer())
    EndIf
    EnableAliasRef(RadarModule)
    EnableAliasRef(RadarSparksMarker)
    StartTimer(iModuleSceneTimerLength as Float, iModuleSceneTimeID)
EndFunction

Function AdvanceModuleSequence()
    If !IsStageDone(317)
        SetStage(317)
    ElseIf !IsStageDone(318)
        SetStage(318)
    ElseIf !IsStageDone(iModuleCompleteStage)
        SetStage(iModuleCompleteStage)
    ElseIf !IsStageDone(330)
        SetStage(330)
    EndIf
EndFunction

Function BeginOrbitalDrop()
    Float delay = iOrbitalDropTimerLength
    If delay <= 0.0
        delay = 5.0
    EndIf
    StartTimer(delay, iOrbitalDropTimerID)
EndFunction

Function SpawnOrbitalDrop()
    Actor playerRef = Game.GetPlayer()
    Form crateBase = Game.GetFormFromFile(0x0029CC0A, "SeventySix.esm")
    Form scanGrenadeBase = Game.GetFormFromFile(0x00052213, "SeventySix.esm")
    Form strikeGrenadeBase = Game.GetFormFromFile(0x0029CC0F, "SeventySix.esm")
    ObjectReference dropRef = None
    If playerRef != None && crateBase != None
        dropRef = playerRef.PlaceAtMe(crateBase, 1, False, False)
    EndIf
    If dropRef != None
        dropRef.MoveTo(playerRef, 180.0, 0.0, 20.0, True)
        If scanGrenadeBase != None
            dropRef.AddItem(scanGrenadeBase, 1, True)
        EndIf
        If strikeGrenadeBase != None
            dropRef.AddItem(strikeGrenadeBase, 1, True)
        EndIf
        If EN02_SmokeBombExplosion != None
            dropRef.PlaceAtMe(EN02_SmokeBombExplosion, 1, False, False)
        EndIf
        ResourceDropContainer.ForceRefTo(dropRef)
        ActiveDropContainer.ForceRefTo(dropRef)
    ElseIf playerRef != None
        If scanGrenadeBase != None
            playerRef.AddItem(scanGrenadeBase, 1, False)
        EndIf
        If strikeGrenadeBase != None
            playerRef.AddItem(strikeGrenadeBase, 1, False)
        EndIf
    EndIf
    If !IsStageDone(350)
        SetStage(350)
    EndIf
EndFunction

Function CheckOrbitalRewards()
    If !IsRunning() || IsCompleted() || !IsStageDone(350) || IsStageDone(360)
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    Form scanGrenadeBase = Game.GetFormFromFile(0x00052213, "SeventySix.esm")
    Form strikeGrenadeBase = Game.GetFormFromFile(0x0029CC0F, "SeventySix.esm")
    If playerRef != None && scanGrenadeBase != None && playerRef.GetItemCount(scanGrenadeBase) > 0 && !IsStageDone(358)
        SetStage(358)
    EndIf
    If playerRef != None && strikeGrenadeBase != None && playerRef.GetItemCount(strikeGrenadeBase) > 0 && !IsStageDone(359)
        SetStage(359)
    EndIf
    If IsStageDone(358) && IsStageDone(359)
        If !IsStageDone(360)
            SetStage(360)
        EndIf
        Return
    EndIf
    Float delay = iBroadcastFailsafeLength as Float
    If delay <= 0.0
        delay = 1.0
    EndIf
    StartTimer(delay, iBroadcastFailsafeID)
EndFunction

Function UpdateCheckpoint(Int aiStage)
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || EN02_CheckpointValue == None
        Return
    EndIf
    If StageArrayContains(ToSGStagesToSet, aiStage)
        playerRef.SetValue(EN02_CheckpointValue, iToSugarGroveAV as Float)
        bToSGCheckpoint = True
    ElseIf StageArrayContains(CollInstStagesToSet, aiStage)
        playerRef.SetValue(EN02_CheckpointValue, iCollectedInstructionsAV as Float)
        bCollInstCheckpoint = True
        bCollectedInstructions = True
    ElseIf StageArrayContains(GivenModStagesToSet, aiStage)
        playerRef.SetValue(EN02_CheckpointValue, iGivenModuleAV as Float)
        bGivenModCheckpoint = True
    ElseIf StageArrayContains(CollectedBeaconStagesToSet, aiStage)
        playerRef.SetValue(EN02_CheckpointValue, iCollectedBeaconsAV as Float)
        bCollBeacCheckpoint = True
    ElseIf StageArrayContains(CommonStagesToSet, aiStage)
        bGeneralCheckpoint = True
    EndIf
EndFunction

Bool Function StageArrayContains(Int[] akStages, Int aiStage)
    If akStages == None
        Return False
    EndIf
    Int i = 0
    While i < akStages.Length
        If akStages[i] == aiStage
            Return True
        EndIf
        i = i + 1
    EndWhile
    Return False
EndFunction

Function ForcePlayerIntoAlias(ReferenceAlias akAlias)
    Actor playerRef = Game.GetPlayer()
    If akAlias != None && playerRef != None && akAlias.GetRef() != playerRef
        akAlias.ForceRefTo(playerRef)
    EndIf
EndFunction

Function EnableAliasRef(ReferenceAlias akAlias)
    If akAlias != None && akAlias.GetRef() != None
        akAlias.GetRef().Enable(False)
    EndIf
EndFunction

Function DisableAliasRef(ReferenceAlias akAlias)
    If akAlias != None && akAlias.GetRef() != None
        akAlias.GetRef().Disable(False)
    EndIf
EndFunction

Function SetCollectionActivation(RefCollectionAlias akCollection, Bool abBlocked)
    If akCollection == None
        Return
    EndIf
    Int i = 0
    While i < akCollection.GetCount()
        ObjectReference targetRef = akCollection.GetAt(i)
        If targetRef != None
            targetRef.BlockActivation(abBlocked)
        EndIf
        i = i + 1
    EndWhile
EndFunction
