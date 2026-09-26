MTNM03_Meditation_QuestScript Function EventScript()
    Quest owner = Self as Quest
    Return owner as MTNM03_Meditation_QuestScript
EndFunction

Function StartSceneOnce(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Function StopScene(Scene akScene)
    If akScene != None && akScene.IsPlaying()
        akScene.Stop()
    EndIf
EndFunction

; The intro scene chains Energy -> Discordant Forces -> Holistic Light -> a looping random-lines scene.
Function StopMeditationScenes()
    StopScene(MTNM03_GuidedMeditationIntroScene)
    StopScene(MTNM03_GuidedMeditationEnergyScene)
    StopScene(MTNM03_GuidedMeditationDiscordantForcesScene)
    StopScene(MTNM03_GuidedMeditationHolisticLightScene)
    StopScene(MTNM03_GuidedMeditationRandomizedLinesScene)
EndFunction

Function ResetEventObjectives()
    Int objective = 10
    While objective <= 100
        SetObjectiveDisplayed(objective, False)
        SetObjectiveCompleted(objective, False)
        SetObjectiveFailed(objective, False)
        objective += 10
    EndWhile
EndFunction

Function FailOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective)
        SetObjectiveFailed(aiObjective, True)
    EndIf
EndFunction

Function FinishEvent(Bool abSuccess)
    StopMeditationScenes()
    FailOpenObjective(10)
    FailOpenObjective(20)
    MTNM03_Meditation_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.EndMeditation(abSuccess, MTNM03_Meditation_BuffMessage)
    EndIf
    If abSuccess
        StartSceneOnce(MTNM03_GuidedMeditationEndScene)
    EndIf
EndFunction

Function Fragment_Stage_0001_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && MTNM03_LLI_SpeakerRepairList != None
        playerRef.AddItem(MTNM03_LLI_SpeakerRepairList, 4)
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    ResetEventObjectives()
    MTNM03_Meditation_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ResetMeditation()
    EndIf
    SetObjectiveDisplayed(10, True, True)
EndFunction

; Obsolete in FO76 (the brazier now sets stage 30 directly); kept as a short prep phase whose objective timer sets 30.
Function Fragment_Stage_0020_Item_00()
    If IsStageDone(30)
        Return
    EndIf
    SetObjectiveCompleted(10, True)
    SetObjectiveDisplayed(20, True, True)
    SetEnabled(Alias_MNTM03_BrazierFlames, True)
EndFunction

Function SetEnabled(ReferenceAlias akAlias, Bool abEnabled)
    If akAlias == None || akAlias.GetReference() == None
        Return
    EndIf
    If abEnabled
        akAlias.GetReference().Enable(False)
    Else
        akAlias.GetReference().Disable(False)
    EndIf
EndFunction

Function Fragment_Stage_0030_Item_00()
    If IsStageDone(9991) || IsStageDone(300)
        Return
    EndIf
    SetObjectiveCompleted(10, True)
    If IsObjectiveDisplayed(20)
        SetObjectiveCompleted(20, True)
    EndIf
    MTNM03_Meditation_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.StartMeditation()
    EndIf
    StartSceneOnce(MTNM03_GuidedMeditationIntroScene)
    If !IsStageDone(40)
        SetStage(40)
    EndIf
EndFunction

Function Fragment_Stage_0040_Item_00()
    If IsStageDone(210)
        Return
    EndIf
    Quest owner = Self as Quest
    DefaultQuestEncounterWaveScript waves = owner as DefaultQuestEncounterWaveScript
    If waves == None || waves.EncounterWaves == None
        Return
    EndIf
    Int waveIndex = 0
    While waveIndex < waves.EncounterWaves.Length
        ; Spawns03 names an alias FO76 filled at runtime; without it the spawner would fall back to the brazier itself.
        ReferenceAlias spawnArea = waves.EncounterWaves[waveIndex].SpawnArea
        If spawnArea != None && spawnArea.GetReference() != None
            waves.StartEncounterWave(waveIndex)
        EndIf
        waveIndex += 1
    EndWhile
EndFunction

Function ReconcileHubs()
    MTNM03_Meditation_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ReconcileHubs()
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    ReconcileHubs()
EndFunction

Function Fragment_Stage_0060_Item_00()
    ReconcileHubs()
EndFunction

Function Fragment_Stage_0070_Item_00()
    ReconcileHubs()
EndFunction

Function Fragment_Stage_0080_Item_00()
    ReconcileHubs()
EndFunction

Function Fragment_Stage_0090_Item_00()
    ReconcileHubs()
EndFunction

Function Fragment_Stage_0100_Item_00()
    ReconcileHubs()
EndFunction

Function Fragment_Stage_0110_Item_00()
    ReconcileHubs()
EndFunction

Function Fragment_Stage_0120_Item_00()
    ReconcileHubs()
EndFunction

; B21:QuestTimer sets this stage when the meditation timer runs out.
Function Fragment_Stage_0210_Item_00()
    If IsStageDone(255) || IsStageDone(300) || IsStageDone(9991)
        Return
    EndIf
    MTNM03_Meditation_QuestScript eventScript = EventScript()
    If !IsStageDone(30) || (eventScript != None && eventScript.CountIntactHubs() <= 0)
        SetStage(300)
    Else
        SetStage(255)
    EndIf
EndFunction

Function Fragment_Stage_0255_Item_00()
    FinishEvent(True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    FinishEvent(False)
EndFunction

Function Fragment_Stage_0500_Item_00()
    StopMeditationScenes()
EndFunction

; B21:ObjectiveTimers sets this stage when the brazier prep objective expires.
Function Fragment_Stage_9991_Item_00()
    If IsStageDone(30)
        Return
    EndIf
    FinishEvent(False)
EndFunction
