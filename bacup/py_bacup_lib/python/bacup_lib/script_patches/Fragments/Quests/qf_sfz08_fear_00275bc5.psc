DefaultQuestEncounterWaveScript Function WaveScript()
    Quest owner = Self as Quest
    Return owner as DefaultQuestEncounterWaveScript
EndFunction

SFZ08_Fear_QuestScript Function EventScript()
    Quest owner = Self as Quest
    Return owner as SFZ08_Fear_QuestScript
EndFunction

Function StartEventScene(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Function StopEventScene(Scene akScene)
    If akScene != None && akScene.IsPlaying()
        akScene.Stop()
    EndIf
EndFunction

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
    SetObjectiveDisplayed(300, False)
    SetObjectiveCompleted(300, False)
    SetObjectiveFailed(300, False)
EndFunction

Function StartHiveWave(String asWaveID, Float afSpawnSeconds)
    DefaultQuestEncounterWaveScript waveScript = WaveScript()
    If waveScript == None
        Return
    EndIf
    Int waveIndex = waveScript.FindEncounterWaveIndex(asWaveID)
    If waveIndex < 0
        Return
    EndIf
    waveScript.StartEncounterWave(waveIndex)

    If afSpawnSeconds > 0.0
        SFZ08_Fear_QuestScript eventScript = EventScript()
        If eventScript != None
            eventScript.ArmWaveStop(waveIndex, afSpawnSeconds)
        EndIf
    EndIf
EndFunction

Function FinishHive(Int aiObjective, Int aiNextStage)
    SetObjectiveCompleted(aiObjective)
    If !IsStageDone(aiNextStage)
        SetStage(aiNextStage)
    EndIf
EndFunction

Function EndEventRun(Bool abFailed)
    If abFailed
        FailAllObjectives()
    Else
        CompleteAllObjectives()
    EndIf

    DefaultQuestEncounterWaveScript waveScript = WaveScript()
    If waveScript != None
        waveScript.StopAllEncounterWaves(False)
    EndIf

    SFZ08_Fear_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ArmStageTimer(1000, 20.0)
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    ResetEventObjectives()
    SetObjectiveDisplayed(10, True, True)
    StartEventScene(SFZ08_Fear_BennettStartLoop)

    SFZ08_Fear_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ArmTalkFailsafe()
    EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
    StopEventScene(SFZ08_Fear_BennettStartLoop)
    StartEventScene(SFZ08_Fear_BennettIntro)
    SetObjectiveCompleted(10)

    SFZ08_Fear_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ArmStageTimer(100, 12.0)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    StopEventScene(SFZ08_Fear_BennettIntro)
    StartEventScene(SFZ08_Fear_BennettBegin)
    SetObjectiveCompleted(10)
    SetObjectiveDisplayed(100, True, True)

    SFZ08_Fear_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ArmStageTimer(110, 20.0)
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    StopEventScene(SFZ08_Fear_BennettBegin)
    StartEventScene(SFZ08_Fear_BennettWaves)
    StartHiveWave("Wave 01", 45.0)
EndFunction

Function Fragment_Stage_0120_Item_00()
    FinishHive(100, 130)
EndFunction

Function Fragment_Stage_0130_Item_00()
    StopEventScene(SFZ08_Fear_BennettWaves)
    StartEventScene(SFZ08_Fear_BeckhamEndWaves)

    SFZ08_Fear_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ArmStageTimer(200, 10.0)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    StopEventScene(SFZ08_Fear_BeckhamEndWaves)
    SetObjectiveDisplayed(200, True, True)

    SFZ08_Fear_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ArmStageTimer(210, 20.0)
    EndIf
EndFunction

Function Fragment_Stage_0210_Item_00()
    StartEventScene(SFZ08_Fear_BennettWaves)
    StartHiveWave("Wave 02", 45.0)
EndFunction

Function Fragment_Stage_0220_Item_00()
    FinishHive(200, 230)
EndFunction

Function Fragment_Stage_0230_Item_00()
    StopEventScene(SFZ08_Fear_BennettWaves)
    StartEventScene(SFZ08_Fear_BeckhamEndWaves)

    SFZ08_Fear_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ArmStageTimer(300, 10.0)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    StopEventScene(SFZ08_Fear_BeckhamEndWaves)
    SetObjectiveDisplayed(300, True, True)

    SFZ08_Fear_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ArmStageTimer(310, 20.0)
    EndIf
EndFunction

Function Fragment_Stage_0310_Item_00()
    StartEventScene(SFZ08_Fear_BennettWaves)
    StartHiveWave("Wave 03", 0.0)
EndFunction

Function Fragment_Stage_0320_Item_00()
    FinishHive(300, 400)
EndFunction

Function Fragment_Stage_0400_Item_00()
    StopEventScene(SFZ08_Fear_BennettWaves)

    DefaultQuestEncounterWaveScript waveScript = WaveScript()
    If waveScript != None
        waveScript.StopAllEncounterWaves(False)
    EndIf
    StartEventScene(SFZ08_Fear_BennettEnd)

    SFZ08_Fear_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ArmStageTimer(500, 12.0)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    EndEventRun(False)
EndFunction

Function Fragment_Stage_1000_Item_00()
    Stop()
EndFunction

Function Fragment_Stage_1100_Item_00()
    EndEventRun(True)
EndFunction

Function Fragment_Stage_1110_Item_00()
    EndEventRun(True)
EndFunction
