; Single-player reconstruction of the "I Am Become Death" (EN07_MQ_Death, 002D0F6B)
; quest controller. The shipped FO76 client PEX is server-stripped: 6423 bytes of
; properties and docstrings with zero function bodies.
;
; FO76 drove stage flow from server events. FO4 has none, so progress that the
; server used to push is resolved locally from bound actor values, bound quest
; forms, and activation of the quest's own alias references.
;
; No helper here returns Int or Float: the native Papyrus compiler used by the
; conversion tests cannot resolve value-type return types of same-script
; functions, so counters are written through the existing script variables.

Actor Function GetLocalQuestPlayer()
    If currentPlayer != None
        Actor aliasPlayer = currentPlayer.GetActorReference()
        If aliasPlayer != None
            Return aliasPlayer
        EndIf
    EndIf
    Return Game.GetPlayer()
EndFunction

ObjectReference Function GetLocalAliasRef(Int aiAliasID)
    ReferenceAlias targetAlias = GetAlias(aiAliasID) as ReferenceAlias
    If targetAlias == None
        Return None
    EndIf
    Return targetAlias.GetReference()
EndFunction

Bool Function HasLocalCompleteCodeSet(ActorValue[] akSiloValues, Actor akPlayer)
    If akPlayer == None || akSiloValues.Length == 0
        Return False
    EndIf
    Int i = 0
    While i < akSiloValues.Length
        If akSiloValues[i] == None || akPlayer.GetValue(akSiloValues[i]) < 1.0
            Return False
        EndIf
        i += 1
    EndWhile
    Return True
EndFunction

Bool Function HasLocalAnyCompleteCodeSet(Actor akPlayer)
    Return HasLocalCompleteCodeSet(SiloAlphaAVs, akPlayer) || \
        HasLocalCompleteCodeSet(SiloBravoAVs, akPlayer) || \
        HasLocalCompleteCodeSet(SiloCharlieAVs, akPlayer)
EndFunction

Bool Function HasLocalCodePieceRange(Actor akPlayer, Int aiFirstFormID)
    If akPlayer == None
        Return False
    EndIf
    Int i = 0
    While i < 8
        Form codePage = Game.GetFormFromFile(aiFirstFormID + i, "SeventySix.esm")
        If codePage != None && akPlayer.GetItemCount(codePage) > 0
            Return True
        EndIf
        i += 1
    EndWhile
    Return False
EndFunction

Bool Function HasLocalAnyCodePiece(Actor akPlayer)
    Return HasLocalCodePieceRange(akPlayer, 0x003DA648) || \
        HasLocalCodePieceRange(akPlayer, 0x004DE233) || \
        HasLocalCodePieceRange(akPlayer, 0x004DE23B) || \
        HasLocalAnyCompleteCodeSet(akPlayer)
EndFunction

; Refreshes iTutorialsCompleted from the bound completion actor values and the
; bound tutorial quests, and mirrors the total onto EN07_Death_CK_TutorialState.
Function RecountLocalTutorials(Actor akPlayer)
    Int completed = 0
    Int i = 0
    While i < TutorialData.Length
        TutorialDatum datum = TutorialData[i]
        Bool done = datum.bComplete
        If !done && akPlayer != None && datum.CompletionValue != None
            done = akPlayer.GetValue(datum.CompletionValue) >= 1.0
        EndIf
        If !done && datum.TutorialQuest != None
            done = datum.TutorialQuest.IsCompleted()
        EndIf
        If done && !datum.bComplete
            datum.bComplete = True
            TutorialData[i] = datum
        EndIf
        If done
            completed += 1
        EndIf
        i += 1
    EndWhile
    iTutorialsCompleted = completed
EndFunction

Function CreditLocalTutorialQuest(Quest akTutorialQuest)
    If akTutorialQuest == None
        Return
    EndIf
    Actor player = GetLocalQuestPlayer()
    Int i = 0
    While i < TutorialData.Length
        TutorialDatum datum = TutorialData[i]
        If datum.TutorialQuest == akTutorialQuest
            If datum.bComplete
                Return
            EndIf
            datum.bComplete = True
            TutorialData[i] = datum
            If player != None && datum.CompletionValue != None
                player.SetValue(datum.CompletionValue, 1.0)
            EndIf
            RecountLocalTutorials(player)
            If TutorialData.Length > 0 && iTutorialsCompleted >= TutorialData.Length \
                    && !GetStageDone(iTutorialsDoneStage)
                SetStage(iTutorialsDoneStage)
            EndIf
            Return
        EndIf
        i += 1
    EndWhile
EndFunction

; The four tutorial stations are ACTI refs pulled into aliases 15-18 through the
; Whitespring Bunker location ref types. Their own quests (004EB0F5, 004EB0C2,
; 004EB0F4, 004EB0F3) are server-stripped too, so activating a station is what
; credits its tutorial here.
Function CreditLocalTutorialStation(ObjectReference akStationRef)
    If akStationRef == None
        Return
    EndIf
    If akStationRef == GetLocalAliasRef(18)
        CreditLocalTutorialQuest(Game.GetFormFromFile(0x004EB0F5, "SeventySix.esm") as Quest)
    ElseIf akStationRef == GetLocalAliasRef(15)
        CreditLocalTutorialQuest(Game.GetFormFromFile(0x004EB0C2, "SeventySix.esm") as Quest)
    ElseIf akStationRef == GetLocalAliasRef(16)
        CreditLocalTutorialQuest(Game.GetFormFromFile(0x004EB0F4, "SeventySix.esm") as Quest)
    ElseIf akStationRef == GetLocalAliasRef(17)
        CreditLocalTutorialQuest(Game.GetFormFromFile(0x004EB0F3, "SeventySix.esm") as Quest)
    EndIf
EndFunction

Function RegisterLocalStations()
    Int i = 15
    While i <= 18
        ObjectReference stationRef = GetLocalAliasRef(i)
        If stationRef != None
            RegisterForRemoteEvent(stationRef, "OnActivate")
        EndIf
        i += 1
    EndWhile
    ObjectReference commandTerminal = GetLocalAliasRef(10)
    If commandTerminal != None
        RegisterForRemoteEvent(commandTerminal, "OnActivate")
    EndIf
EndFunction

Function HandleStage(Int aiStage)
    Actor player = GetLocalQuestPlayer()

    If aiStage == iStartUpStage
        SetObjectiveDisplayed(10)

    ElseIf aiStage == iDoTutorialsIndex
        SetObjectiveCompleted(10)
        SetObjectiveDisplayed(iDoTutorialsIndex)
        SetObjectiveDisplayed(iBypassTutorialsIndex)
        If player != None && EN07_Death_CK_TutorialState != None
            player.SetValue(EN07_Death_CK_TutorialState, 1.0)
        EndIf
        RegisterLocalStations()

    ElseIf aiStage == iBypassTutorialsIndex
        SetObjectiveCompleted(iBypassTutorialsIndex)
        SetObjectiveDisplayed(iDoTutorialsIndex, False)
        If player != None && EN07_Death_CK_TutorialState != None
            player.SetValue(EN07_Death_CK_TutorialState, 2.0)
        EndIf
        SetStage(30)

    ElseIf aiStage == iTutorialsDoneStage
        SetObjectiveCompleted(iDoTutorialsIndex)
        SetObjectiveDisplayed(iBypassTutorialsIndex, False)
        SetObjectiveDisplayed(27)
        If player != None && EN07_Death_CK_TutorialState != None
            player.SetValue(EN07_Death_CK_TutorialState, 2.0)
        EndIf

    ElseIf aiStage == 30
        SetObjectiveCompleted(27)
        SetObjectiveDisplayed(iBypassTutorialsIndex, False)
        If player != None && EN07_Death_CK_TutorialState != None
            player.SetValue(EN07_Death_CK_TutorialState, 2.0)
        EndIf
        Quest nukeCodes = Game.GetFormFromFile(0x003DA647, "SeventySix.esm") as Quest
        Keyword nukeCodesStart = Game.GetFormFromFile(0x003DC9BA, "SeventySix.esm") as Keyword
        If nukeCodes != None && !nukeCodes.IsRunning() && !nukeCodes.IsCompleted() && nukeCodesStart != None
            nukeCodesStart.SendStoryEventAndWait(None, player, player)
        EndIf
        If player != None && Nuke_LaunchCard != None && player.GetItemCount(Nuke_LaunchCard) > 0
            SetStage(iLaunchCardAcquiredEarly)
        EndIf
        SetStage(iLaunchCardTrackingOn)
        SetStage(iQuestLogStage)
        SetObjectiveDisplayed(iLaunchCardIndex)
        SetObjectiveDisplayed(iCodePieceIndex)
        SetObjectiveDisplayed(iArchivesIndex)
        If HasLocalAnyCodePiece(player)
            SetStage(iAllCodePiecesAcquired)
        EndIf
        If player != None && EN07_Death_CK_AcquiredMamaDolcePW != None && \
                player.GetValue(EN07_Death_CK_AcquiredMamaDolcePW) >= 1.0
            SetStage(iAlreadyReadArchives)
        EndIf
        If player != None && EN07_Death_CK_FoundKeywordIntel != None && \
                player.GetValue(EN07_Death_CK_FoundKeywordIntel) >= 1.0
            SetStage(iAlreadyReadDolces)
        EndIf

    ElseIf aiStage == iLaunchCardIndex
        SetObjectiveCompleted(iLaunchCardIndex)

    ElseIf aiStage == iAllCodePiecesAcquired
        SetStage(iAcquiredCodePiece)

    ElseIf aiStage == iAcquiredCodePiece
        SetObjectiveCompleted(iCodePieceIndex)

    ElseIf aiStage == iAlreadyReadArchives
        SetStage(iArchivesIndex)

    ElseIf aiStage == iArchivesIndex
        SetObjectiveCompleted(iArchivesIndex)
        SetObjectiveDisplayed(iToDolcesIndex)

    ElseIf aiStage == iCardSearchIndex
        SetObjectiveDisplayed(iCardSearchIndex)

    ElseIf aiStage == 67
        SetObjectiveCompleted(iCardSearchIndex)

    ElseIf aiStage == iAlreadyReadDolces
        SetStage(iToDolcesIndex)

    ElseIf aiStage == iToDolcesIndex
        SetObjectiveCompleted(iCardSearchIndex)
        SetObjectiveCompleted(iToDolcesIndex)

    ElseIf aiStage == iUnlockNukeLaunchQTs
        SetObjectiveDisplayed(iLaunchNukeIndex)
        Quest nukeMaster = Game.GetFormFromFile(0x002D0F67, "SeventySix.esm") as Quest
        If nukeMaster != None && !nukeMaster.IsRunning() && !nukeMaster.IsCompleted()
            nukeMaster.Start()
        EndIf

    ElseIf aiStage == iCompletionStage
        SetObjectiveCompleted(iLaunchNukeIndex)

    ElseIf aiStage == iJumpToFinishStage
        SetStage(iCompletionStage)
    EndIf
EndFunction

Function EvaluateLocalProgress()
    If !IsRunning() || IsCompleted()
        Return
    EndIf
    Actor player = GetLocalQuestPlayer()

    If player != None && EN07_Death_LaunchedNuke != None && \
            player.GetValue(EN07_Death_LaunchedNuke) >= 1.0 && !GetStageDone(iCompletionStage)
        SetStage(iCompletionStage)
        Return
    EndIf

    If GetStageDone(30)
        Quest nukeCodes = Game.GetFormFromFile(0x003DA647, "SeventySix.esm") as Quest
        Keyword nukeCodesStart = Game.GetFormFromFile(0x003DC9BA, "SeventySix.esm") as Keyword
        If nukeCodes != None && !nukeCodes.IsRunning() && !nukeCodes.IsCompleted() && nukeCodesStart != None
            nukeCodesStart.SendStoryEventAndWait(None, player, player)
        EndIf
    EndIf

    If GetStageDone(iUnlockNukeLaunchQTs) && !GetStageDone(iCompletionStage)
        HandleStage(iUnlockNukeLaunchQTs)
    EndIf

    If GetStageDone(iDoTutorialsIndex) && !GetStageDone(iTutorialsDoneStage)
        RecountLocalTutorials(player)
        If TutorialData.Length > 0 && iTutorialsCompleted >= TutorialData.Length
            SetStage(iTutorialsDoneStage)
        EndIf
    EndIf

    If GetStageDone(iLaunchCardTrackingOn) && !GetStageDone(iAcquiredCodePiece) && \
            !GetStageDone(iAllCodePiecesAcquired) && HasLocalAnyCodePiece(player)
        SetStage(iAllCodePiecesAcquired)
    EndIf

    If GetStageDone(iLaunchCardIndex) && GetStageDone(iAcquiredCodePiece) && \
            GetStageDone(iToDolcesIndex) && !GetStageDone(iUnlockNukeLaunchQTs)
        If SetStage(iUnlockNukeLaunchQTs)
            HandleStage(iUnlockNukeLaunchQTs)
        EndIf
    EndIf
EndFunction

Event OnQuestInit()
    bProcessing = False
    If EN07_MQ_Nuke_Master != None && !EN07_MQ_Nuke_Master.IsRunning() && \
            !EN07_MQ_Nuke_Master.IsCompleted()
        EN07_MQ_Nuke_Master.Start()
    EndIf
    If Nuke_Master != None && !Nuke_Master.IsRunning() && !Nuke_Master.IsCompleted()
        Nuke_Master.Start()
    EndIf
    Actor player = GetLocalQuestPlayer()
    RecountLocalTutorials(player)

    If player != None && EN07_Death_LaunchedNuke != None && \
            player.GetValue(EN07_Death_LaunchedNuke) >= 1.0
        SetStage(iJumpToFinishStage)
        Return
    EndIf

    RegisterLocalStations()
    If !GetStageDone(iStartUpStage)
        SetStage(iStartUpStage)
    EndIf
    StartTimer(5.0, 7201)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 7201
        Return
    EndIf
    If !IsRunning() || IsCompleted()
        Return
    EndIf
    If !bProcessing
        bProcessing = True
        EvaluateLocalProgress()
        bProcessing = False
    EndIf
    If IsRunning() && !IsCompleted()
        StartTimer(5.0, 7201)
    EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef != GetLocalQuestPlayer() || !IsRunning() || IsCompleted()
        Return
    EndIf

    If akSender == GetLocalAliasRef(10)
        If GetStageDone(iTutorialsDoneStage) && !GetStageDone(30)
            SetStage(30)
        EndIf
        Return
    EndIf

    If GetStageDone(iDoTutorialsIndex) && !GetStageDone(iTutorialsDoneStage)
        CreditLocalTutorialStation(akSender)
    EndIf
EndEvent
