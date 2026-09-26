Actor Function MTNS01_GetPlayer()
    Actor playerRef = None
    If Alias_currentPlayer != None
        playerRef = Alias_currentPlayer.GetActorReference()
    EndIf
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    Return playerRef
EndFunction

Function MTNS01_SetCheckpoint(Int aiCheckpoint)
    Actor playerRef = MTNS01_GetPlayer()
    If playerRef != None && MTNS01_CheckpointValue != None
        If (playerRef.GetValue(MTNS01_CheckpointValue) as Int) < aiCheckpoint
            playerRef.SetValue(MTNS01_CheckpointValue, aiCheckpoint as Float)
        EndIf
    EndIf
EndFunction

MTNS01QuestScript Function MTNS01_GetController()
    Return (Self as Quest) as MTNS01QuestScript
EndFunction

Function Fragment_Stage_0050_Item_00()
    Actor playerRef = MTNS01_GetPlayer()
    If playerRef != None && MTN_RDR_MQ_StartedValue != None
        playerRef.SetValue(MTN_RDR_MQ_StartedValue, 1.0)
    EndIf
    SetObjectiveDisplayed(50)
    If MTNS01_Rose_Loudspeaker_StartScene != None && !MTNS01_Rose_Loudspeaker_StartScene.IsPlaying()
        MTNS01_Rose_Loudspeaker_StartScene.Start()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveDisplayed(100)
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(50)
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(200)
    MTNS01_SetCheckpoint(1)

    MTNS01QuestScript controller = MTNS01_GetController()
    If controller != None
        controller.MTNS01_MoveQuestItem(Schematics, Alias_Corpse)
    EndIf
EndFunction

Function Fragment_Stage_0201_Item_00()
    SetObjectiveCompleted(200)
    If !IsStageDone(205)
        SetStage(205)
    Else
        SetObjectiveDisplayed(205)
    EndIf
EndFunction

Function Fragment_Stage_0205_Item_00()
    SetObjectiveDisplayed(205)

    MTNS01QuestScript controller = MTNS01_GetController()
    If controller != None
        controller.MTNS01_MoveQuestItem(Alias_RespondersNote, Alias_Corpse)
    EndIf
EndFunction

Function Fragment_Stage_0206_Item_00()
    SetObjectiveCompleted(200)
    SetObjectiveCompleted(205)
    If !IsStageDone(210)
        SetStage(210)
    EndIf
EndFunction

Function Fragment_Stage_0210_Item_00()
    SetObjectiveCompleted(200)
    SetObjectiveCompleted(205)
    SetObjectiveDisplayed(211)
    SetObjectiveDisplayed(212)
    SetObjectiveDisplayed(213)
    MTNS01_SetCheckpoint(2)

    MTNS01QuestScript controller = MTNS01_GetController()
    If controller != None
        controller.MTNS01_MoveQuestItem(Alias_RadioParts01, Alias_Container01)
        controller.MTNS01_MoveQuestItem(Alias_RadioParts02, Alias_Container02)
    EndIf
EndFunction

Function Fragment_Stage_0211_Item_00()
    SetObjectiveCompleted(211)
    MTNS01_SetCheckpoint(3)
    If (IsStageDone(213) || IsStageDone(225)) && !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0213_Item_00()
    SetObjectiveCompleted(213)
    MTNS01_SetCheckpoint(4)
    If (IsStageDone(211) || IsStageDone(220)) && !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0220_Item_00()
    SetObjectiveCompleted(211)
    SetObjectiveDisplayed(212)
    SetObjectiveDisplayed(213)
    MTNS01_SetCheckpoint(3)

    MTNS01QuestScript controller = MTNS01_GetController()
    If controller != None
        controller.MTNS01_MoveQuestItem(Alias_RadioParts02, Alias_Container02)
    EndIf
EndFunction

Function Fragment_Stage_0225_Item_00()
    SetObjectiveDisplayed(211)
    SetObjectiveDisplayed(212)
    SetObjectiveCompleted(213)
    MTNS01_SetCheckpoint(4)

    MTNS01QuestScript controller = MTNS01_GetController()
    If controller != None
        controller.MTNS01_MoveQuestItem(Alias_RadioParts01, Alias_Container01)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(211)
    SetObjectiveCompleted(212)
    SetObjectiveCompleted(213)
    SetObjectiveDisplayed(300)
    MTNS01_SetCheckpoint(5)
EndFunction

Function Fragment_Stage_0400_Item_00()
    SetObjectiveCompleted(300)
    SetObjectiveDisplayed(400)
    MTNS01_SetCheckpoint(6)
EndFunction

Function Fragment_Stage_0401_Item_00()
    MTNS01_StartAmbush()
    SetObjectiveCompleted(400)
    SetObjectiveDisplayed(410)
EndFunction

Function Fragment_Stage_0410_Item_00()
    Actor playerRef = MTNS01_GetPlayer()
    SetObjectiveCompleted(410)
    SetObjectiveDisplayed(420)

    If playerRef != None && PlayerRepeaterInstalled != None
        PlayerRepeaterInstalled.ForceRefTo(playerRef)
    EndIf
    If QSTMTNS01RepeaterInstall != None && Alias_RepeaterInstallSoundMarker != None
        ObjectReference soundMarker = Alias_RepeaterInstallSoundMarker.GetReference()
        If soundMarker != None
            QSTMTNS01RepeaterInstall.Play(soundMarker)
        EndIf
    EndIf
EndFunction

Function Fragment_Stage_0420_Item_00()
    Actor playerRef = MTNS01_GetPlayer()
    SetObjectiveCompleted(420)
    If playerRef != None && PlayerSignalBoosted != None
        PlayerSignalBoosted.ForceRefTo(playerRef)
    EndIf
    If !IsStageDone(500)
        SetStage(500)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    SetObjectiveCompleted(400)
    SetObjectiveCompleted(410)
    SetObjectiveCompleted(420)
    SetObjectiveDisplayed(500)
    MTNS01_SetCheckpoint(7)
EndFunction

Function Fragment_Stage_0600_Item_00()
    SetObjectiveCompleted(500)
    CompleteAllObjectives()
    If MTNS01_TryStartMayhem()
        Stop()
    Else
        StartTimer(5.0, 600)
    EndIf
EndFunction

Bool Function MTNS01_TryStartMayhem()
    Quest nextQuest = Game.GetFormFromFile(0x0009732E, "SeventySix.esm") as Quest
    If nextQuest != None && (nextQuest.IsRunning() || nextQuest.IsCompleted())
        Return True
    EndIf
    Actor playerRef = MTNS01_GetPlayer()
    If nextQuest == None || playerRef == None || MTNM01_Mayhem_Quest_Keyword == None
        Return False
    EndIf
    Bool accepted = MTNM01_Mayhem_Quest_Keyword.SendStoryEventAndWait(None, playerRef, playerRef)
    Return accepted || nextQuest.IsRunning() || nextQuest.IsCompleted()
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 401
        MTNS01_StartAmbush()
        Return
    EndIf
    If aiTimerID != 600 || !IsRunning() || !IsStageDone(600)
        Return
    EndIf
    If MTNS01_TryStartMayhem()
        Stop()
    Else
        StartTimer(5.0, 600)
    EndIf
EndEvent

Function MTNS01_StartAmbush()
    If !IsRunning() || !IsStageDone(401) || GetStage() >= 420
        CancelTimer(401)
        Return
    EndIf
    B21:LocalEncounterMaterializer materializer = (Self as Quest) as B21:LocalEncounterMaterializer
    If materializer != None
        materializer.PrepareEligibleWaves()
        If !materializer.HasPreparedWave(0)
            StartTimer(5.0, 401)
            Return
        EndIf
    EndIf
    RefCollectionAlias enemies = GetAlias(101) as RefCollectionAlias
    DefaultQuestEncounterWaveScript waves = (Self as Quest) as DefaultQuestEncounterWaveScript
    If enemies == None || enemies.GetCount() == 0 || waves == None
        StartTimer(5.0, 401)
        Return
    EndIf
    waves.StartLocalEncounterWave(0)
    CancelTimer(401)
EndFunction

Event OnQuestInit()
    RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
    MTNS01_StartAmbush()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    MTNS01_StartAmbush()
EndEvent

Event OnQuestShutdown()
    CancelTimer(401)
    UnregisterForAllRemoteEvents()
EndEvent
