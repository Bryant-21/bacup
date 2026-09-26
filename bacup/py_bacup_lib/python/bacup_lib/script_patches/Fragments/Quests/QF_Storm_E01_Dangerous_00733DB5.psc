Bool Function IsEventOver()
    Return IsStageDone(9000) || IsStageDone(9997) || IsStageDone(9998) || IsStageDone(9999)
EndFunction

Bool Function OtherEndStageDone(Int aiStage)
    Return (aiStage != 9000 && IsStageDone(9000)) || (aiStage != 9997 && IsStageDone(9997)) || (aiStage != 9998 && IsStageDone(9998)) || (aiStage != 9999 && IsStageDone(9999))
EndFunction

Storm_E01_Dangerous Function EventScript()
    Quest owner = Self as Quest
    Return owner as Storm_E01_Dangerous
EndFunction

Storm_E01_LightningStrikes Function StrikeScript()
    Quest owner = Self as Quest
    Return owner as Storm_E01_LightningStrikes
EndFunction

DefaultQuestEncounterWaveScript Function WaveScript()
    Quest owner = Self as Quest
    Return owner as DefaultQuestEncounterWaveScript
EndFunction

Function StartEventWave(String asWaveID)
    DefaultQuestEncounterWaveScript waves = WaveScript()
    If waves != None
        waves.StartEncounterWaveByID(asWaveID)
    EndIf
EndFunction

Function StopEventWave(String asWaveID)
    DefaultQuestEncounterWaveScript waves = WaveScript()
    If waves != None
        waves.StopEncounterWaveByID(asWaveID, False)
    EndIf
EndFunction

Function StartEventScene(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Function ResetEventObjective(Int aiObjective)
    SetObjectiveDisplayed(aiObjective, False)
    SetObjectiveCompleted(aiObjective, False)
    SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetEventObjectives()
    ResetEventObjective(10)
    ResetEventObjective(20)
    ResetEventObjective(30)
    ResetEventObjective(40)
    ResetEventObjective(50)
    ResetEventObjective(60)
    ResetEventObjective(70)
EndFunction

Function CompleteOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveCompleted(aiObjective, True)
    EndIf
EndFunction

Function FailOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveFailed(aiObjective, True)
    EndIf
EndFunction

Function FailOpenObjectives()
    FailOpenObjective(10)
    FailOpenObjective(20)
    FailOpenObjective(30)
    FailOpenObjective(40)
    FailOpenObjective(50)
    FailOpenObjective(60)
EndFunction

Function StopChargeWaves()
    StopEventWave("LostChargeWave")
    StopEventWave("LostChargeWave2")
    StopEventWave("LostChargeWaveTrees")
    StopEventWave("ChargedLostWave")
EndFunction

Function FinishEvent(Int aiEndStage, Scene akResultScene)
    If OtherEndStageDone(aiEndStage)
        Return
    EndIf
    If aiEndStage != 9000
        FailOpenObjectives()
    EndIf
    DefaultQuestEncounterWaveScript waves = WaveScript()
    If waves != None
        waves.StopAllEncounterWaves(False)
    EndIf
    Storm_E01_Dangerous eventScript = EventScript()
    If eventScript != None
        eventScript.StopIntercomWait()
        eventScript.StopChargeTracking()
        eventScript.EndPartCollection()
    EndIf
    Storm_E01_LightningStrikes strikes = StrikeScript()
    If strikes != None
        strikes.StopChargedStrikes()
    EndIf
    StartEventScene(akResultScene)
    Quest owner = Self as Quest
    If (owner as B21:ObjectiveTimers) != None
        ; Objective 70 is the hidden Storm_E01_ShutdownTimer that sets 10000.
        SetObjectiveDisplayed(70, True)
    ElseIf !IsStageDone(10000)
        SetStage(10000)
    EndIf
EndFunction

Function Fragment_Stage_0000_Item_00()
    ResetEventObjectives()
    Storm_E01_LightningStrikes strikes = StrikeScript()
    If strikes != None
        strikes.ResetStrikeState()
    EndIf
    SetObjectiveDisplayed(10, True, True)
    Storm_E01_Dangerous eventScript = EventScript()
    If eventScript != None
        eventScript.BeginIntercomWait()
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    Storm_E01_Dangerous eventScript = EventScript()
    If eventScript != None
        eventScript.StopIntercomWait()
    EndIf
    CompleteOpenObjective(10)
    If !IsEventOver() && !IsStageDone(150)
        SetStage(150)
    EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
    If IsEventOver()
        Return
    EndIf
    SetObjectiveDisplayed(20, True, True)
    StartEventScene(Storm_E01_DangerousCollectionSceneStart)
EndFunction

Function Fragment_Stage_0200_Item_00()
    CompleteOpenObjective(20)
    If IsEventOver()
        Return
    EndIf
    SetObjectiveDisplayed(30, True, True)
    Storm_E01_Dangerous eventScript = EventScript()
    If eventScript != None
        eventScript.BeginPartCollection()
    EndIf
    StartEventWave("LostInstallWave")
    StartEventWave("LostInstallWaveTrees")
    StartEventScene(Storm_E01_Dangerous_CollectionScene)
EndFunction

Function Fragment_Stage_0250_Item_00()
    CompleteOpenObjective(30)
    StopEventWave("LostInstallWave")
    StopEventWave("LostInstallWaveTrees")
    Storm_E01_Dangerous eventScript = EventScript()
    If eventScript != None
        eventScript.EndPartCollection()
    EndIf
    If IsEventOver()
        Return
    EndIf
    SetObjectiveDisplayed(40, True, True)
    StartEventScene(Storm_E01_DangerousCollectionSceneComplete)
EndFunction

Function Fragment_Stage_0300_Item_00()
    CompleteOpenObjective(40)
    If IsEventOver()
        Return
    EndIf
    SetObjectiveDisplayed(50, True, True)
    StartEventScene(Storm_E01_DangerousChargeSceneStart)
    StartEventWave("LostChargeWave")
    StartEventWave("LostChargeWave2")
    StartEventWave("LostChargeWaveTrees")
    StartEventWave("ChargedLostWave")
    If !IsStageDone(305)
        SetStage(305)
    EndIf
    If !IsStageDone(310)
        SetStage(310)
    EndIf
EndFunction

Function Fragment_Stage_0305_Item_00()
    If IsEventOver() || IsStageDone(365)
        Return
    EndIf
    Storm_E01_Dangerous eventScript = EventScript()
    If eventScript != None
        eventScript.StartChargeTracking()
    EndIf
EndFunction

Function Fragment_Stage_0310_Item_00()
    If IsEventOver() || IsStageDone(370)
        Return
    EndIf
    Storm_E01_LightningStrikes strikes = StrikeScript()
    If strikes != None
        strikes.StartChargedStrikes()
    EndIf
EndFunction

Function Fragment_Stage_0360_Item_00()
    CompleteOpenObjective(50)
    StopChargeWaves()
    If IsEventOver()
        Return
    EndIf
    StartEventScene(Storm_E01_DangerousChargeSceneComplete)
    If !IsStageDone(365)
        SetStage(365)
    EndIf
    If !IsStageDone(370)
        SetStage(370)
    EndIf
EndFunction

Function Fragment_Stage_0365_Item_00()
    Storm_E01_Dangerous eventScript = EventScript()
    If eventScript != None
        eventScript.StopChargeTracking()
    EndIf
EndFunction

Function Fragment_Stage_0370_Item_00()
    Storm_E01_LightningStrikes strikes = StrikeScript()
    If strikes != None
        strikes.StopChargedStrikes()
    EndIf
    If IsEventOver()
        Return
    EndIf
    If strikes != None
        strikes.ActivateBoss()
    EndIf
    If !IsStageDone(400)
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
    If IsEventOver()
        Return
    EndIf
    SetObjectiveDisplayed(60, True, True)
    StartEventWave("LostBossWave")
    StartEventWave("LostBossWave2")
    StartEventWave("LostBossWaveTrees")
EndFunction

Function Fragment_Stage_9000_Item_00()
    CompleteOpenObjective(60)
    FinishEvent(9000, Storm_E01_DangerousEventEndSuccess)
EndFunction

Function Fragment_Stage_9997_Item_00()
    FinishEvent(9997, Storm_E01_DangerousEventEndFailCollect)
EndFunction

Function Fragment_Stage_9998_Item_00()
    FinishEvent(9998, Storm_E01_DangerousEventEndFailCharge)
EndFunction

Function Fragment_Stage_9999_Item_00()
    FinishEvent(9999, Storm_E01_DangerousEventEndFailBoss)
EndFunction

Function Fragment_Stage_10000_Item_00()
    CompleteOpenObjective(70)
    Storm_E01_LightningStrikes strikes = StrikeScript()
    If strikes != None
        strikes.StopAllStrikes()
    EndIf
    Stop()
EndFunction

Function Fragment_Stage_10001_Item_00()
    Storm_E01_Dangerous eventScript = EventScript()
    If eventScript != None
        eventScript.StopIntercomWait()
        eventScript.StopChargeTracking()
        eventScript.EndPartCollection()
    EndIf
    Storm_E01_LightningStrikes strikes = StrikeScript()
    If strikes != None
        strikes.StopAllStrikes()
    EndIf
EndFunction
