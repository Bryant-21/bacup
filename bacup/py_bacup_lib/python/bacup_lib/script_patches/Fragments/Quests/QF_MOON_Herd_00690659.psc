MOON_Herd_QuestScript Function EventScript()
    Quest owner = Self as Quest
    Return owner as MOON_Herd_QuestScript
EndFunction

Function StartSceneIfIdle(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Function CompleteObjectiveIfOpen(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveCompleted(aiObjective, True)
    EndIf
EndFunction

Function CloseOpenObjectives(Bool abFailed)
    Int[] objectives = New Int[11]
    objectives[0] = 10
    objectives[1] = 20
    objectives[2] = 30
    objectives[3] = 40
    objectives[4] = 50
    objectives[5] = 60
    objectives[6] = 70
    objectives[7] = 90
    objectives[8] = 100
    objectives[9] = 110
    objectives[10] = 120
    Int index = 0
    While index < objectives.Length
        Int objective = objectives[index]
        If IsObjectiveDisplayed(objective) && !IsObjectiveCompleted(objective) && !IsObjectiveFailed(objective)
            If abFailed
                SetObjectiveFailed(objective, True)
            Else
                SetObjectiveCompleted(objective, True)
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Function SetCollectionEnabled(RefCollectionAlias akCollection, Bool abEnabled)
    Int index = 0
    While akCollection != None && index < akCollection.GetCount()
        ObjectReference member = akCollection.GetAt(index)
        If member != None
            If abEnabled
                member.EnableNoWait()
            Else
                member.DisableNoWait()
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

; Each cabin group reports through DefaultCollectionAliasOnDeath; a group that never
; filled would hold the kill stage forever, so it counts as already cleared.
Function ReleaseCritterGroup(RefCollectionAlias akCritters, Int aiClearedStage)
    If akCritters == None || akCritters.GetCount() == 0
        SetStage(aiClearedStage)
        Return
    EndIf
    Int living = 0
    Int index = 0
    While index < akCritters.GetCount()
        Actor critter = akCritters.GetAt(index) as Actor
        If critter != None
            critter.EnableNoWait()
            If Actor_AllCritters != None && Actor_AllCritters.Find(critter) < 0
                Actor_AllCritters.AddRef(critter)
            EndIf
            If !critter.IsDead()
                living += 1
            EndIf
        EndIf
        index += 1
    EndWhile
    If living == 0
        SetStage(aiClearedStage)
    EndIf
EndFunction

Function SetWaveGroupRunning(Int aiWave, Bool abRunning)
    MOON_Herd_QuestScript eventScript = EventScript()
    If eventScript == None
        Return
    EndIf
    String[] waveIDs
    If aiWave == 1
        waveIDs = New String[11]
        waveIDs[0] = "W1_Wolves_N_Noise"
        waveIDs[1] = "W1_Wolves_E_Noise"
        waveIDs[2] = "W1_Wolves_S2_Noise"
        waveIDs[3] = "W1_Radscorpions_C_Player"
        waveIDs[4] = "W1_Other1_S1_Noise"
        waveIDs[5] = "W1_Other1_W_Noise"
        waveIDs[6] = "W1_Other1_N_Noise"
        waveIDs[7] = "W1_FloatersFire_N_Player"
        waveIDs[8] = "W1_FloatersIce_S2_Player"
        waveIDs[9] = "W1_FloatersIce_W_Player"
        waveIDs[10] = "W1_Wolves_C_Player"
    ElseIf aiWave == 2
        waveIDs = New String[9]
        waveIDs[0] = "W2_Wolves_N_Noise"
        waveIDs[1] = "W2_Wolves_E_Noise"
        waveIDs[2] = "W2_Wolves_S1_Noise"
        waveIDs[3] = "W2_Radscorpions_C_Player"
        waveIDs[4] = "W2_Other2_S1_Noise"
        waveIDs[5] = "W2_Other2_W_Noise"
        waveIDs[6] = "W2_Other2_N_Noise"
        waveIDs[7] = "W2_FloatersFire_E_Player"
        waveIDs[8] = "W2_Wolves_C_Player"
    Else
        waveIDs = New String[15]
        waveIDs[0] = "Wolves_East"
        waveIDs[1] = "Wolves_West"
        waveIDs[2] = "Wolves_South2"
        waveIDs[3] = "Floaters_North"
        waveIDs[4] = "Floaters_East"
        waveIDs[5] = "Floaters_South2"
        waveIDs[6] = "Floaters_West"
        waveIDs[7] = "Radscorpions_West"
        waveIDs[8] = "Radscorpions_South2"
        waveIDs[9] = "Yaogaui_East"
        waveIDs[10] = "Yaogaui_South1"
        waveIDs[11] = "Yaogaui_West"
        waveIDs[12] = "Honeybeast_South1"
        waveIDs[13] = "RadToad_North"
        waveIDs[14] = "Mirelurks_North"
    EndIf
    Int index = 0
    While index < waveIDs.Length
        If abRunning
            eventScript.StartEventWave(waveIDs[index])
        Else
            eventScript.StopEventWave(waveIDs[index])
        EndIf
        index += 1
    EndWhile
EndFunction

Function PlayRepellerMalfunction()
    ObjectReference tower = None
    If Alias_RepellerTower != None
        tower = Alias_RepellerTower.GetReference()
    EndIf
    If tower == None
        Return
    EndIf
    If Sound_WaveSpeaker != None
        Sound_WaveSpeaker.Play(tower)
    EndIf
    If Sound_Wolves != None
        Sound_Wolves.Play(tower)
    EndIf
EndFunction

Function PulseRepeller()
    ObjectReference tower = None
    If Alias_RepellerTower != None
        tower = Alias_RepellerTower.GetReference()
    EndIf
    If tower != None
        If VFX_SoundWave != None
            tower.PlaceAtMe(VFX_SoundWave)
        EndIf
        If Sound_Repeller != None
            Sound_Repeller.Play(tower)
        EndIf
    EndIf
    MOON_Herd_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.PulseRepellerOnEnemies(Actor_WaveEnemies_All)
        eventScript.PulseRepellerOnEnemies(Actor_WaveEnemies_Generator)
        eventScript.PulseRepellerOnEnemies(Actor_WaveEnemies_Players)
    EndIf
EndFunction

Function ShowDefenceMessage()
    If Message_Defence != None
        Message_Defence.Show()
    EndIf
EndFunction

Function SetTerminalBlocked(Bool abBlocked)
    If Alias_Terminal == None || Alias_Terminal.GetReference() == None || BlockPlayerActivationKeyword == None
        Return
    EndIf
    If abBlocked
        Alias_Terminal.GetReference().AddKeyword(BlockPlayerActivationKeyword)
    Else
        Alias_Terminal.GetReference().RemoveKeyword(BlockPlayerActivationKeyword)
    EndIf
EndFunction

Function FailEvent(Scene akReactionScene)
    CloseOpenObjectives(True)
    MOON_Herd_QuestScript eventScript = EventScript()
    If eventScript == None
        SetStage(5000)
        Return
    EndIf
    eventScript.CancelEventTimers()
    eventScript.StopAllEventWaves(False)
    eventScript.SetStageAfterScene(akReactionScene, 5000, 60.0)
EndFunction

Function ResetEventWorld()
    MOON_Herd_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.CleanupEvent()
    EndIf
    SetCollectionEnabled(Actor_AllCritters, False)
    SetCollectionEnabled(Alias_Fences, False)
    SetTerminalBlocked(False)
    If Alias_Generator != None && Alias_Generator.GetReference() != None
        Alias_Generator.GetReference().ClearDestruction()
    EndIf
EndFunction

Function Fragment_Stage_0000_Item_00()
    MOON_Herd_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.InitializeEvent()
    EndIf
    SetCollectionEnabled(Alias_Fences, True)
    SetTerminalBlocked(False)
    If Alias_Generator != None && Alias_Generator.GetReference() != None
        Alias_Generator.GetReference().ClearDestruction()
    EndIf
    SetStage(100)
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True, True)
    MOON_Herd_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.BeginTalkFallback(Alias_Vera, 200)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    MOON_Herd_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.EndTalkFallback()
    EndIf
    CompleteObjectiveIfOpen(10)
    SetObjectiveDisplayed(20, True, True)
    SetObjectiveDisplayed(30, True, True)
    StartSceneIfIdle(Scene_VRadio_Stage_Kill)
    ReleaseCritterGroup(Actor_Wolf, 210)
    ReleaseCritterGroup(Actor_HoneyBeast, 220)
    ReleaseCritterGroup(Actor_Beeswarm, 230)
    ReleaseCritterGroup(Actor_Yaoguai, 240)
    ReleaseCritterGroup(Actor_Radscorpion, 350)
EndFunction

Function Fragment_Stage_0210_Item_00()
    CheckCabinsCleared()
EndFunction

Function Fragment_Stage_0220_Item_00()
    CheckCabinsCleared()
EndFunction

Function Fragment_Stage_0230_Item_00()
    CheckCabinsCleared()
EndFunction

Function Fragment_Stage_0240_Item_00()
    CheckCabinsCleared()
EndFunction

Function CheckCabinsCleared()
    If IsStageDone(210) && IsStageDone(220) && IsStageDone(230) && IsStageDone(240)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    If IsStageDone(350)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
    If IsStageDone(300)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    CompleteObjectiveIfOpen(30)
    SetObjectiveDisplayed(40, True, True)
    SetTerminalBlocked(True)
    StartSceneIfIdle(Scene_VRadio_Stage_Fuses)
    MOON_Herd_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.SetTextVariableForCollection(Alias_Repellers, 0)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    CompleteObjectiveIfOpen(20)
    CompleteObjectiveIfOpen(40)
    SetObjectiveDisplayed(50, True, True)
    SetObjectiveDisplayed(110, True, True)
    SetWaveGroupRunning(1, True)
    PlayRepellerMalfunction()
    StartSceneIfIdle(Scene_VRadio_Stage_Wave1)
    SetStage(510)
EndFunction

Function Fragment_Stage_0510_Item_00()
    ShowDefenceMessage()
EndFunction

Function Fragment_Stage_0540_Item_00()
    CompleteObjectiveIfOpen(50)
    SetWaveGroupRunning(1, False)
    PulseRepeller()
    SetStage(550)
EndFunction

Function Fragment_Stage_0550_Item_00()
    SetObjectiveDisplayed(90, True, True)
    StartSceneIfIdle(Scene_VRadio_Stage_Intermission1)
EndFunction

Function Fragment_Stage_0600_Item_00()
    CompleteObjectiveIfOpen(90)
    SetObjectiveDisplayed(60, True, True)
    SetWaveGroupRunning(2, True)
    PlayRepellerMalfunction()
    StartSceneIfIdle(Scene_VRadio_Stage_Wave2)
    SetStage(610)
EndFunction

Function Fragment_Stage_0610_Item_00()
    ShowDefenceMessage()
EndFunction

Function Fragment_Stage_0640_Item_00()
    CompleteObjectiveIfOpen(60)
    SetWaveGroupRunning(2, False)
    PulseRepeller()
    SetStage(650)
EndFunction

Function Fragment_Stage_0650_Item_00()
    SetObjectiveDisplayed(100, True, True)
    StartSceneIfIdle(Scene_VRadio_Stage_Intermission2)
EndFunction

Function Fragment_Stage_0690_Item_00()
    CompleteObjectiveIfOpen(100)
    ShowDefenceMessage()
    SetStage(700)
EndFunction

Function Fragment_Stage_0700_Item_00()
    SetObjectiveDisplayed(70, True, True)
    SetWaveGroupRunning(3, True)
    PlayRepellerMalfunction()
    StartSceneIfIdle(Scene_VRadio_Stage_Wave3)
    MOON_Herd_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ScheduleBlueDevil()
    EndIf
EndFunction

Function Fragment_Stage_0750_Item_00()
    If IsStageDone(950)
        Return
    EndIf
    ObjectReference tower = None
    If Alias_RepellerTower != None
        tower = Alias_RepellerTower.GetReference()
    EndIf
    If Sound_BD != None && tower != None
        Sound_BD.Play(tower)
    EndIf
    MOON_Herd_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.StartEventWave("WaveBoss_Cryptid")
    EndIf
    SetObjectiveDisplayed(120, True, True)
    StartSceneIfIdle(Scene_VRadio_Stage_WaveBD)
EndFunction

Function Fragment_Stage_0800_Item_00()
    If IsStageDone(2200)
        Return
    EndIf
    CompleteObjectiveIfOpen(120)
    SetStage(950)
EndFunction

Function Fragment_Stage_0950_Item_00()
    If IsStageDone(2200)
        Return
    EndIf
    CompleteObjectiveIfOpen(70)
    CompleteObjectiveIfOpen(110)
    CompleteObjectiveIfOpen(120)
    MOON_Herd_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.CancelEventTimers()
        eventScript.StopAllEventWaves(False)
    EndIf
    PulseRepeller()
    If eventScript != None
        eventScript.SetCircuitBreakersFixed(True)
        eventScript.SetStageAfterScene(None, 1000, 5.0)
    Else
        SetStage(1000)
    EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
    CloseOpenObjectives(False)
    MOON_Herd_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.SetStageAfterScene(Scene_VRadio_React_Victory, 5000, 60.0)
    Else
        SetStage(5000)
    EndIf
EndFunction

Function Fragment_Stage_2000_Item_00()
    If IsStageDone(200)
        Return
    EndIf
    FailEvent(Scene_VRadio_React_Failure1)
EndFunction

Function Fragment_Stage_2100_Item_00()
    If IsStageDone(500)
        Return
    EndIf
    FailEvent(Scene_VRadio_React_Failure2)
EndFunction

Function Fragment_Stage_2200_Item_00()
    If IsStageDone(950)
        Return
    EndIf
    FailEvent(Scene_VRadio_React_Failure3)
EndFunction

Function Fragment_Stage_5000_Item_00()
    ResetEventWorld()
    Stop()
EndFunction

Function Fragment_Stage_6000_Item_00()
    ResetEventWorld()
EndFunction
