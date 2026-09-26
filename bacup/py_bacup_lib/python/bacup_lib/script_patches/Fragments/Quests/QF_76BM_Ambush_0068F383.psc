Moon_Ambush_QuestScript Function EventScript()
    Quest owner = Self as Quest
    Return owner as Moon_Ambush_QuestScript
EndFunction

Function StartSceneIfIdle(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

; DialogueScenes[0] is MOON_Ambush_Commentary_EventKickoff, the scene Luca's greeting starts.
Scene Function KickoffScene()
    If DialogueScenes == None || DialogueScenes.Length == 0
        Return None
    EndIf
    Return DialogueScenes[0]
EndFunction

Function CompleteObjectiveIfOpen(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveCompleted(aiObjective, True)
    EndIf
EndFunction

Function FailObjectiveIfOpen(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveFailed(aiObjective, True)
    EndIf
EndFunction

Function CloseOpenObjectives(Bool abFailed)
    Int[] objectives = New Int[9]
    objectives[0] = 20
    objectives[1] = 25
    objectives[2] = 30
    objectives[3] = 35
    objectives[4] = 40
    objectives[5] = 50
    objectives[6] = 70
    objectives[7] = 80
    objectives[8] = 100
    Int index = 0
    While index < objectives.Length
        If abFailed
            FailObjectiveIfOpen(objectives[index])
        Else
            CompleteObjectiveIfOpen(objectives[index])
        EndIf
        index += 1
    EndWhile
EndFunction

Function SetExplosiveWavesRunning(Moon_Ambush_QuestScript akEventScript, Bool abRunning)
    If akEventScript == None
        Return
    EndIf
    String[] waveIDs = New String[7]
    waveIDs[0] = "ExplosiveCultistsWave01"
    waveIDs[1] = "ExplosiveCultistsWave02"
    waveIDs[2] = "ExplosiveCultistsWave03"
    waveIDs[3] = "ExplosiveCultistsWave04"
    waveIDs[4] = "ExplosiveCultistBossWave01"
    waveIDs[5] = "ExplosiveCultistBossWave02"
    waveIDs[6] = "ExplosiveCultistBossWave03"
    Int index = 0
    While index < waveIDs.Length
        If abRunning
            akEventScript.StartEventWave(waveIDs[index])
        Else
            akEventScript.StopEventWave(waveIDs[index])
        EndIf
        index += 1
    EndWhile
EndFunction

Function SetCargoWavesRunning(Moon_Ambush_QuestScript akEventScript, Bool abRunning)
    If akEventScript == None
        Return
    EndIf
    String[] waveIDs = New String[6]
    waveIDs[0] = "CargoCultistWave01"
    waveIDs[1] = "CargoCultistWave02"
    waveIDs[2] = "CargoCultistWave03"
    waveIDs[3] = "CargoCultistWave04"
    waveIDs[4] = "HighUpCultistWave01"
    waveIDs[5] = "HighUpCultistWave02"
    Int index = 0
    While index < waveIDs.Length
        If abRunning
            akEventScript.StartEventWave(waveIDs[index])
        Else
            akEventScript.StopEventWave(waveIDs[index])
        EndIf
        index += 1
    EndWhile
EndFunction

Function SetPackBrahminEnabled(Bool abEnabled)
    If Alias_PackBrahmin == None || Alias_PackBrahmin.GetReference() == None
        Return
    EndIf
    If abEnabled
        Alias_PackBrahmin.GetReference().Enable(False)
    Else
        Alias_PackBrahmin.GetReference().DisableNoWait()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Moon_Ambush_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ResetBombs()
        eventScript.BeginTalkFallback(Alias_Luca, KickoffScene(), 150)
    EndIf
    SetObjectiveDisplayed(20, True, True)
EndFunction

Function Fragment_Stage_0150_Item_00()
    Moon_Ambush_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.EndTalkFallback()
    EndIf
    CompleteObjectiveIfOpen(20)
    SetObjectiveDisplayed(25, True, True)
    StartSceneIfIdle(MOON_Ambush_Commentary_Prep_Start)
EndFunction

Function Fragment_Stage_0175_Item_00()
    CompleteObjectiveIfOpen(25)
    SetObjectiveDisplayed(30, True, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    CompleteObjectiveIfOpen(30)
    SetObjectiveDisplayed(35, True, True)
    SetObjectiveDisplayed(40, True, True)
    StartSceneIfIdle(MOON_Ambush_Commentary_CampApproach_Start)
    Moon_Ambush_QuestScript eventScript = EventScript()
    SetExplosiveWavesRunning(eventScript, True)
    If eventScript != None
        eventScript.BeginExplosivePhase()
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    CompleteObjectiveIfOpen(35)
    CompleteObjectiveIfOpen(40)
    SetObjectiveDisplayed(50, True, True)
    StartSceneIfIdle(MOON_Ambush_Commentary_PlaceExplosives_End)
EndFunction

Function Fragment_Stage_0350_Item_00()
    CompleteObjectiveIfOpen(50)
    Moon_Ambush_QuestScript eventScript = EventScript()
    SetExplosiveWavesRunning(eventScript, False)
    StartSceneIfIdle(Moon_Ambush_Commentary_Explosion_End)
    If eventScript != None
        eventScript.DetonateBombs()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    CompleteObjectiveIfOpen(50)
    SetPackBrahminEnabled(True)
    Moon_Ambush_CartScript cart = Alias_PackBrahmin as Moon_Ambush_CartScript
    If cart != None
        cart.BeginCargoLoading()
    EndIf
    SetObjectiveDisplayed(70, True, True)
    SetObjectiveDisplayed(80, True, True)
    SetCargoWavesRunning(EventScript(), True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    StartSceneIfIdle(MOON_Ambush_Commentary_Cargo_Actions)
EndFunction

Function Fragment_Stage_0600_Item_00()
    CompleteObjectiveIfOpen(70)
    CompleteObjectiveIfOpen(80)
    Moon_Ambush_QuestScript eventScript = EventScript()
    SetCargoWavesRunning(eventScript, False)
    If eventScript != None
        eventScript.RemoveEventItemsFromPlayer()
        eventScript.SetStageAfterScene(MOON_Ambush_Commentary_Cargo_End, 650, 30.0)
    EndIf
EndFunction

Function Fragment_Stage_0650_Item_00()
    Moon_Ambush_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.SpawnOgua()
    EndIf
    SetObjectiveDisplayed(100, True, True)
    StartSceneIfIdle(MOON_Ambush_Commentary_OguaSpawn)
EndFunction

Function Fragment_Stage_0700_Item_00()
    CompleteObjectiveIfOpen(100)
    SetStage(4000)
EndFunction

Function Fragment_Stage_4000_Item_00()
    If IsStageDone(8500) || IsStageDone(9000)
        Return
    EndIf
    If IsStageDone(700)
        SetStage(9000)
    Else
        SetStage(8500)
    EndIf
EndFunction

Function Fragment_Stage_8500_Item_00()
    CloseOpenObjectives(True)
    Moon_Ambush_QuestScript eventScript = EventScript()
    Scene endingScene = MOON_Ambush_Commentary_EventFail_NoOgua
    If IsStageDone(650)
        endingScene = MOON_Ambush_Commentary_EventFail_Ogua
    EndIf
    StartSceneIfIdle(Moon_Ambush_FailSafe_Timer)
    If eventScript != None
        eventScript.StopAllEventWaves(False)
        eventScript.SetStageAfterScene(endingScene, 9999, 90.0)
    Else
        SetStage(9999)
    EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
    CloseOpenObjectives(False)
    Moon_Ambush_QuestScript eventScript = EventScript()
    StartSceneIfIdle(Moon_Ambush_FailSafe_Timer)
    If eventScript != None
        eventScript.StopAllEventWaves(False)
        eventScript.SetStageAfterScene(MOON_Ambush_Commentary_EventSucceed, 9999, 90.0)
    Else
        SetStage(9999)
    EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
    Moon_Ambush_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.StopAllEventWaves(True)
        eventScript.CleanupEvent()
    EndIf
    SetPackBrahminEnabled(False)
    Stop()
EndFunction
