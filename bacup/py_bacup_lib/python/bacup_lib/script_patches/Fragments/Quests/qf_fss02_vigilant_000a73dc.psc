DefaultQuestEncounterWaveScript Function WaveScript()
    Quest owner = Self as Quest
    Return owner as DefaultQuestEncounterWaveScript
EndFunction

FSS02_Vigilant_QuestScript Function EventScript()
    Quest owner = Self as Quest
    Return owner as FSS02_Vigilant_QuestScript
EndFunction

FSS02_Vigilant_RoverAliasScript Function RoverScript()
    Return Alias_Rover as FSS02_Vigilant_RoverAliasScript
EndFunction

Function SetTerminalState(Float afState)
    If FSS02_Vigilant_TerminalGlobal != None
        FSS02_Vigilant_TerminalGlobal.SetValue(afState)
    EndIf
EndFunction

Function FailEventRun()
    FailAllObjectives()
    SetTerminalState(0.0)

    DefaultQuestEncounterWaveScript waveScript = WaveScript()
    If waveScript != None
        waveScript.StopAllEncounterWaves(False)
    EndIf

    FSS02_Vigilant_RoverAliasScript roverScript = RoverScript()
    If roverScript != None
        roverScript.StopScanningFX()
        roverScript.BeginPodReturn()
    EndIf
    If FSS02_Vigilant_RoverToPodScene != None && !FSS02_Vigilant_RoverToPodScene.IsPlaying()
        FSS02_Vigilant_RoverToPodScene.Start()
    EndIf

    FSS02_Vigilant_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ArmEventShutdown()
    EndIf
EndFunction

Function Fragment_Stage_0010_Item_00()
    FSS02_Vigilant_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ResetEventObjectives()
        eventScript.PublishLocationGlobal()
    EndIf
    SetTerminalState(1.0)
    SetObjectiveDisplayed(100, True, True)
    If !IsStageDone(50)
        SetStage(50)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(100, True, True)
    If FSS02_Vigilant_RoverFixScene && !FSS02_Vigilant_RoverFixScene.IsPlaying()
        FSS02_Vigilant_RoverFixScene.Start()
    EndIf

    FSS02_Vigilant_RoverAliasScript roverScript = RoverScript()
    If roverScript != None
        roverScript.ArmRepairApproachPoll()
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    SetObjectiveCompleted(100)
    SetObjectiveDisplayed(200, True, True)

    FSS02_Vigilant_RoverAliasScript roverScript = RoverScript()
    If roverScript != None
        roverScript.CancelRepairWindow()
        roverScript.StartScanningFX()
    EndIf
    If !IsStageDone(205)
        SetStage(205)
    EndIf
EndFunction

Function Fragment_Stage_0205_Item_00()
    DefaultQuestEncounterWaveScript waveScript = WaveScript()
    If waveScript != None
        waveScript.StartEncounterWave(0)
    EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
    SetObjectiveCompleted(200)

    DefaultQuestEncounterWaveScript waveScript = WaveScript()
    If waveScript != None
        waveScript.StopAllEncounterWaves(False)
    EndIf
    FSS02_Vigilant_RoverAliasScript roverScript = RoverScript()
    If roverScript != None
        roverScript.StopScanningFX()
    EndIf

    If FSS02_Vigilant_RoverToPodScene && !FSS02_Vigilant_RoverToPodScene.IsPlaying()
        FSS02_Vigilant_RoverToPodScene.Start()
    EndIf
    If !IsStageDone(300)
        SetStage(300)
    EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
    SetObjectiveCompleted(200)
    SetObjectiveDisplayed(300, True, True)
    SetTerminalState(2.0)
EndFunction

Function Fragment_Stage_1000_Item_00()
    SetObjectiveCompleted(300)
    SetTerminalState(0.0)

    DefaultQuestEncounterWaveScript waveScript = WaveScript()
    If waveScript != None
        waveScript.StopAllEncounterWaves(False)
    EndIf
    FSS02_Vigilant_QuestScript eventScript = EventScript()
    If eventScript != None
        eventScript.ArmEventShutdown()
    EndIf
EndFunction

Function Fragment_Stage_9990_Item_00()
    FailEventRun()
EndFunction

Function Fragment_Stage_9991_Item_00()
    FailEventRun()
EndFunction

Function Fragment_Stage_10000_Item_00()
    SetTerminalState(0.0)
    If FSS02_Vigilant_LocationGlobal != None
        FSS02_Vigilant_LocationGlobal.SetValue(0.0)
    EndIf

    DefaultQuestEncounterWaveScript waveScript = WaveScript()
    If waveScript != None
        waveScript.StopAllEncounterWaves(True)
    EndIf
    Stop()
EndFunction
