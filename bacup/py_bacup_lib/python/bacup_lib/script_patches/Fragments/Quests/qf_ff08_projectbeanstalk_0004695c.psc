Function ResetEventObjectives()
    SetObjectiveDisplayed(10, False)
    SetObjectiveCompleted(10, False)
    SetObjectiveFailed(10, False)
    SetObjectiveDisplayed(100, False)
    SetObjectiveCompleted(100, False)
    SetObjectiveFailed(100, False)
    SetObjectiveDisplayed(200, False)
    SetObjectiveCompleted(200, False)
    SetObjectiveFailed(200, False)
    SetObjectiveDisplayed(225, False)
    SetObjectiveDisplayed(300, False)
    SetObjectiveCompleted(300, False)
    SetObjectiveDisplayed(400, False)
    SetObjectiveCompleted(400, False)
    SetObjectiveDisplayed(500, False)
    SetObjectiveCompleted(500, False)
    SetObjectiveDisplayed(600, False)
EndFunction

FF08_BeanstalkQuest Function EventScript()
    Return (Self as Quest) as FF08_BeanstalkQuest
EndFunction

DefaultQuestEncounterWaveScript Function WaveSystem()
    Return (Self as Quest) as DefaultQuestEncounterWaveScript
EndFunction

Function StartSprayWave(String asWaveID)
    DefaultQuestEncounterWaveScript waveSystem = WaveSystem()
    If waveSystem != None
        waveSystem.StartEncounterWaveByID(asWaveID)
    EndIf
EndFunction

Function StopSprayWave(String asWaveID)
    DefaultQuestEncounterWaveScript waveSystem = WaveSystem()
    If waveSystem != None
        waveSystem.StopEncounterWaveByID(asWaveID, False)
    EndIf
EndFunction

Function StopAllSprayWaves()
    DefaultQuestEncounterWaveScript waveSystem = WaveSystem()
    If waveSystem != None
        waveSystem.StopAllEncounterWaves(True)
    EndIf
EndFunction

Actor Function PharmabotActor()
    If QuestGiver == None
        Return None
    EndIf
    Return QuestGiver.GetActorReference()
EndFunction

Function SetPharmabotGhosted(Bool abGhosted)
    Actor botRef = PharmabotActor()
    If botRef != None
        botRef.SetGhost(abGhosted)
    EndIf
EndFunction

Function WakePharmabotWorldCopy()
    ; PF_FF09_ShutDown disables the bot when the experiment scene parks it in the
    ; barn, so a repeat run has to bring the world copy back before initialization.
    Actor botRef = PharmabotActor()
    If botRef != None && botRef.IsDisabled()
        botRef.Enable(False)
    EndIf
EndFunction

Function SetPharmabotDormant(Bool abDormant)
    Actor botRef = PharmabotActor()
    If botRef != None && botRef.IsUnconscious() != abDormant
        botRef.SetUnconscious(abDormant)
    EndIf
EndFunction

Function RepairPharmabot()
    Actor botRef = PharmabotActor()
    If botRef != None
        botRef.ResetHealthAndLimbs()
    EndIf
    SetObjectiveDisplayed(600, False)
EndFunction

Function StartSceneIfIdle(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Function AbortToFailScene()
    StopAllSprayWaves()
    SetPharmabotGhosted(True)
    RepairPharmabot()
    If FailScene && !FailScene.IsPlaying()
        FailScene.Start()
    EndIf
EndFunction

Function ShutdownEvent()
    StopAllSprayWaves()
    SetObjectiveDisplayed(225, False)
    SetObjectiveDisplayed(600, False)
    SetPharmabotGhosted(False)
    SetPharmabotDormant(False)
    Stop()
EndFunction

Function Fragment_Stage_0005_Item_00()
    ResetEventObjectives()
    FF08_BeanstalkQuest eventScript = EventScript()
    If eventScript != None
        eventScript.ResetSprayedLocations()
    EndIf
    WakePharmabotWorldCopy()
    SetPharmabotGhosted(True)
    SetPharmabotDormant(True)
    SetObjectiveDisplayed(10, True, True)
EndFunction

Function Fragment_Stage_0010_Item_00()
    SetObjectiveCompleted(10, True)
    SetPharmabotDormant(False)
    StartSceneIfIdle(InitScene)
EndFunction

Function Fragment_Stage_0015_Item_00()
    SetObjectiveDisplayed(100, True, True)
    SetObjectiveDisplayed(225, True, True)
    StartSceneIfIdle(ExperimentScene)
EndFunction

Function Fragment_Stage_0030_Item_00()
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True, True)
    SetObjectiveDisplayed(225, True, True)
    SetObjectiveDisplayed(300, True, True)
    SetPharmabotGhosted(False)
    StartSprayWave("Wave 1")
EndFunction

Function Fragment_Stage_0035_Item_00()
    SetObjectiveCompleted(300, True)
    FF08_BeanstalkQuest eventScript = EventScript()
    If eventScript != None
        eventScript.AddSprayedLocation()
    EndIf
EndFunction

Function Fragment_Stage_0040_Item_00()
    StartSprayWave("Wave 2")
    StopSprayWave("Wave 1")
    SetObjectiveDisplayed(400, True, True)
EndFunction

Function Fragment_Stage_0045_Item_00()
    SetObjectiveCompleted(400, True)
    FF08_BeanstalkQuest eventScript = EventScript()
    If eventScript != None
        eventScript.AddSprayedLocation()
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    SetObjectiveDisplayed(500, True, True)
    StartSprayWave("Wave 3")
    StopSprayWave("Wave 2")
EndFunction

Function Fragment_Stage_0055_Item_00()
    StopSprayWave("Wave 3")
    StopAllSprayWaves()
    SetObjectiveCompleted(500, True)
    FF08_BeanstalkQuest eventScript = EventScript()
    If eventScript != None
        eventScript.AddSprayedLocation()
    EndIf
    SetObjectiveCompleted(200, True)
    SetObjectiveDisplayed(600, False)
    SetPharmabotGhosted(True)
EndFunction

Function Fragment_Stage_0090_Item_00()
    If !IsObjectiveCompleted(200)
        SetObjectiveFailed(200, True)
    EndIf
    AbortToFailScene()
EndFunction

Function Fragment_Stage_0097_Item_00()
    If !IsObjectiveCompleted(200)
        SetObjectiveFailed(200, True)
    EndIf
    AbortToFailScene()
EndFunction

Function Fragment_Stage_0098_Item_00()
    If !IsObjectiveCompleted(10)
        SetObjectiveFailed(10, True)
    EndIf
    ShutdownEvent()
EndFunction

Function Fragment_Stage_0099_Item_00()
    ShutdownEvent()
EndFunction

Function Fragment_Stage_0100_Item_00()
    ShutdownEvent()
EndFunction
