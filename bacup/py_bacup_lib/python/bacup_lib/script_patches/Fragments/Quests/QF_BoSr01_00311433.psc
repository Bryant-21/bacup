BoSr01_Script Function EventScript()
    Quest owner = Self as Quest
    Return owner as BoSr01_Script
EndFunction

DefaultQuestEncounterWaveScript Function WaveScript()
    Quest owner = Self as Quest
    Return owner as DefaultQuestEncounterWaveScript
EndFunction

; Invaders from Beyond swaps each scorched round for its alien override while its global is on.
Int Function TakeoverWaveIndex(Int aiRegularIndex)
    Quest owner = Self as Quest
    Quests:E07B_Invaders:EventTakeoverScript takeover = owner as Quests:E07B_Invaders:EventTakeoverScript
    If takeover == None || takeover.LCP_E07B_Invaders == None || takeover.LCP_E07B_Invaders.GetValue() <= 0.0 || takeover.EncounterWaveIndices == None
        Return aiRegularIndex
    EndIf
    Int index = 0
    While index < takeover.EncounterWaveIndices.Length
        If takeover.EncounterWaveIndices[index] != None && takeover.EncounterWaveIndices[index].RegularEWSIndex == aiRegularIndex && takeover.EncounterWaveIndices[index].AlienEWSIndex >= 0
            Return takeover.EncounterWaveIndices[index].AlienEWSIndex
        EndIf
        index += 1
    EndWhile
    Return aiRegularIndex
EndFunction

Function StartDefenseRound(String asWaveID)
    DefaultQuestEncounterWaveScript waves = WaveScript()
    If waves == None
        Return
    EndIf
    Int waveIndex = waves.FindEncounterWaveIndex(asWaveID)
    If waveIndex >= 0
        waves.StartEncounterWave(TakeoverWaveIndex(waveIndex))
    EndIf
EndFunction

Function StartScene(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Function StopScene(Scene akScene)
    If akScene != None && akScene.IsPlaying()
        akScene.Stop()
    EndIf
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

Function HideObjectiveIfOpen(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveDisplayed(aiObjective, False)
    EndIf
EndFunction

Function EndDefense(Bool abFailed)
    If abFailed
        FailObjectiveIfOpen(50)
        FailObjectiveIfOpen(100)
        FailObjectiveIfOpen(200)
        FailObjectiveIfOpen(300)
        FailObjectiveIfOpen(325)
        FailObjectiveIfOpen(350)
        FailObjectiveIfOpen(400)
        FailObjectiveIfOpen(500)
        FailObjectiveIfOpen(600)
    Else
        CompleteObjectiveIfOpen(325)
        CompleteObjectiveIfOpen(400)
        CompleteObjectiveIfOpen(500)
        CompleteObjectiveIfOpen(600)
        HideObjectiveIfOpen(350)
    EndIf
    HideObjectiveIfOpen(700)
    HideObjectiveIfOpen(800)
    DefaultQuestEncounterWaveScript waves = WaveScript()
    If waves != None
        waves.StopAllEncounterWaves(False)
    EndIf
    BoSr01_Script eventScript = EventScript()
    If eventScript != None
        eventScript.ScheduleShutdown()
    EndIf
EndFunction

Function Fragment_Stage_0001_Item_00()
    BoSr01_Script eventScript = EventScript()
    If eventScript != None
        eventScript.PrepareGenerator()
    EndIf
    ; The event is listed in the Pip-Boy from its start stage; stage 100 keeps these objectives in step with the generator.
    SetObjectiveDisplayed(50, True)
    SetObjectiveDisplayed(100, True)
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0002_Item_00()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None && pTEST_LL_BoSR01_SonicGRepair != None
        playerRef.AddItem(pTEST_LL_BoSR01_SonicGRepair, 1, False)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    If IsStageDone(300)
        Return
    EndIf
    SetObjectiveDisplayed(50, True)
    If IsStageDone(200)
        SetObjectiveCompleted(100, True)
        SetObjectiveDisplayed(200, True)
    Else
        SetObjectiveDisplayed(100, True)
    EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
    BoSr01_Script eventScript = EventScript()
    If eventScript != None
        eventScript.UpdateGeneratorState()
    EndIf
    If !IsStageDone(100) || IsStageDone(300)
        Return
    EndIf
    SetObjectiveCompleted(100, True)
    SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
    CompleteObjectiveIfOpen(100)
    SetObjectiveCompleted(200, True)
    If IsStageDone(325)
        Return
    EndIf
    SetObjectiveDisplayed(300, True)
    StartScene(pBoSr01_300_PreCombatCheck)
    BoSr01_Script eventScript = EventScript()
    If eventScript != None
        eventScript.StartPreCombatFallback()
    EndIf
EndFunction

Function Fragment_Stage_0325_Item_00()
    CompleteObjectiveIfOpen(300)
    CompleteObjectiveIfOpen(50)
    StopScene(pBoSr01_300_PreCombatCheck)
    SetObjectiveDisplayed(325, True)
    SetObjectiveDisplayed(400, True)
    StartScene(pBoSr01_325_FirstWaveInc)
    StartDefenseRound("Wave 1: Scorched")
    DefaultQuestEncounterWaveScript waves = WaveScript()
    If waves != None
        waves.StartEncounterWaveByID("Scorchbeast")
    EndIf
    BoSr01_Script eventScript = EventScript()
    If eventScript != None
        eventScript.BeginDefense()
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    If !IsStageDone(325) || IsStageDone(700) || IsStageDone(8900) || IsStageDone(9900)
        Return
    EndIf
    SetObjectiveCompleted(400, True)
    SetObjectiveDisplayed(500, True)
    StartScene(pBoSr01_400_SecondWaveInc)
    StartDefenseRound("Wave 2: Scorched")
EndFunction

Function Fragment_Stage_0500_Item_00()
    If !IsStageDone(325) || IsStageDone(700) || IsStageDone(8900) || IsStageDone(9900)
        Return
    EndIf
    SetObjectiveCompleted(500, True)
    SetObjectiveDisplayed(600, True)
    StartScene(pBoSr01_500_ThirdWaveInc)
    StartDefenseRound("Wave 3: Scorched")
EndFunction

Function Fragment_Stage_0600_Item_00()
    If !IsStageDone(325) || IsStageDone(700) || IsStageDone(8900) || IsStageDone(9900)
        Return
    EndIf
    SetObjectiveCompleted(600, True)
    StartScene(pBoSr01_600_QuestOver)
    SetStage(700)
EndFunction

Function Fragment_Stage_0700_Item_00()
    If IsStageDone(8900) || IsStageDone(9900)
        Return
    EndIf
    EndDefense(False)
EndFunction

Function Fragment_Stage_8900_Item_00()
    If IsStageDone(700) || IsStageDone(9900)
        Return
    EndIf
    EndDefense(True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    StopScene(pBoSr01_300_PreCombatCheck)
    StopScene(pBoSr01_325_FirstWaveInc)
    StopScene(pBoSr01_400_SecondWaveInc)
    StopScene(pBoSr01_500_ThirdWaveInc)
    StopScene(pBoSr01_600_QuestOver)
    BoSr01_Script eventScript = EventScript()
    If eventScript != None
        eventScript.CancelDefenseTimers()
    EndIf
EndFunction

Function Fragment_Stage_9900_Item_00()
    If IsStageDone(700) || IsStageDone(8900)
        Return
    EndIf
    EndDefense(True)
EndFunction
