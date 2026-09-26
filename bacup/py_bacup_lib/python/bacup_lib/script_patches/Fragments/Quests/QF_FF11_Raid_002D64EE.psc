DefaultQuestEncounterWaveScript Function GetWaveScript()
    Quest owner = Self as Quest
    Return owner as DefaultQuestEncounterWaveScript
EndFunction

B21:QuestTimer Function GetEventTimer()
    Quest owner = Self as Quest
    Return owner as B21:QuestTimer
EndFunction

B21:QuestVariables Function GetEventVariables()
    Quest owner = Self as Quest
    Return owner as B21:QuestVariables
EndFunction

FF11_Raid_AirDropScript Function GetAirDropScript()
    Quest owner = Self as Quest
    Return owner as FF11_Raid_AirDropScript
EndFunction

FF11_Raid_QuestScript Function GetRaidScript()
    Quest owner = Self as Quest
    Return owner as FF11_Raid_QuestScript
EndFunction

Bool Function IsEventResolved()
    Return IsStageDone(9000) || IsStageDone(9991) || IsStageDone(9992)
EndFunction

Function ResetEventObjective(Int aiObjective)
    SetObjectiveDisplayed(aiObjective, False)
    SetObjectiveCompleted(aiObjective, False)
    SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetEventObjectives()
    ResetEventObjective(5)
    ResetEventObjective(10)
    ResetEventObjective(15)
    ResetEventObjective(18)
    ResetEventObjective(20)
    ResetEventObjective(25)
    ResetEventObjective(30)
    ResetEventObjective(35)
    ResetEventObjective(40)
    ResetEventObjective(45)
    ResetEventObjective(50)
    ResetEventObjective(55)
    ResetEventObjective(60)
    ResetEventObjective(65)
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

Function CompleteFightObjectives()
    CompleteOpenObjective(18)
    CompleteOpenObjective(25)
    CompleteOpenObjective(35)
    CompleteOpenObjective(45)
    CompleteOpenObjective(55)
    CompleteOpenObjective(65)
EndFunction

Function FailEventObjectives()
    FailOpenObjective(5)
    FailOpenObjective(10)
    FailOpenObjective(18)
    FailOpenObjective(20)
    FailOpenObjective(25)
    FailOpenObjective(30)
    FailOpenObjective(35)
    FailOpenObjective(40)
    FailOpenObjective(45)
    FailOpenObjective(50)
    FailOpenObjective(55)
    FailOpenObjective(60)
    FailOpenObjective(65)
    FailOpenObjective(70)
EndFunction

Function PublishWaveCount(Int aiWaveNumber)
    B21:QuestVariables eventVariables = GetEventVariables()
    If eventVariables != None
        eventVariables.SetVariable("TotalWaves", 5.0)
        eventVariables.SetVariable("WavesCurrent", aiWaveNumber as Float)
    EndIf
EndFunction

Function SetSirenEnabled(Bool abEnabled)
    If FF11_Raid_AirRaidSirenMarker == None
        Return
    EndIf
    If abEnabled
        FF11_Raid_AirRaidSirenMarker.Enable(False)
    Else
        FF11_Raid_AirRaidSirenMarker.Disable(False)
    EndIf
EndFunction

Function SetAirportRespawnEnabled(Bool abEnabled)
    ; The airfield's ambient respawn spheres are held off while the landing zone fight runs.
    SetRespawnTriggerEnabled(MorgAir_RespawnActorGroupTrigger_01, abEnabled)
    SetRespawnTriggerEnabled(MorgAir_RespawnActorGroupTrigger_02, abEnabled)
    SetRespawnTriggerEnabled(MorgAir_RespawnActorGroupTrigger_03, abEnabled)
EndFunction

Function SetRespawnTriggerEnabled(ObjectReference akTrigger, Bool abEnabled)
    If akTrigger == None
        Return
    EndIf
    If abEnabled
        akTrigger.Enable(False)
    Else
        akTrigger.Disable(False)
    EndIf
EndFunction

Function ShowIntroMessage()
    Actor playerRef = Game.GetPlayer()
    If FF11_ScorchedIncMessage == None || playerRef == None
        Return
    EndIf
    If FF11_HeardIntroMessage != None && playerRef.GetValue(FF11_HeardIntroMessage) > 0.0
        Return
    EndIf
    FF11_ScorchedIncMessage.Show()
    If FF11_HeardIntroMessage != None
        playerRef.SetValue(FF11_HeardIntroMessage, 1.0)
    EndIf
EndFunction

Function BroadcastEventTopic(Topic akTopic)
    Quest owner = Self as Quest
    DefaultQuestEmergencyBroadcastScript broadcastScript = owner as DefaultQuestEmergencyBroadcastScript
    If broadcastScript != None
        broadcastScript.SendEmergencyBroadcast(akTopic, None)
    EndIf
EndFunction

Function StartScorchedWave(String asWaveID, Int aiWaveNumber, Int aiFightObjective, Int aiBreakObjective)
    If IsEventResolved()
        Return
    EndIf
    If aiBreakObjective > 0
        CompleteOpenObjective(aiBreakObjective)
    EndIf
    PublishWaveCount(aiWaveNumber)
    SetObjectiveDisplayed(10, True)
    SetObjectiveDisplayed(aiFightObjective, True, True)
    DefaultQuestEncounterWaveScript waveScript = GetWaveScript()
    If waveScript != None
        waveScript.StartEncounterWaveByID(asWaveID)
    EndIf
EndFunction

Function BeginWaveBreak(Int aiFightObjective, Int aiBreakObjective)
    If IsEventResolved()
        Return
    EndIf
    CompleteOpenObjective(aiFightObjective)
    ; Displaying the break objective arms its B21:ObjectiveTimers row, which sets the next wave stage.
    SetObjectiveDisplayed(aiBreakObjective, True, True)
EndFunction

Function StopScorchedWaves()
    DefaultQuestEncounterWaveScript waveScript = GetWaveScript()
    If waveScript != None
        waveScript.StopAllEncounterWaves(False)
    EndIf
EndFunction

Function StopEventTimer()
    B21:QuestTimer eventTimer = GetEventTimer()
    If eventTimer != None
        eventTimer.StopQuestTimer()
    EndIf
EndFunction

Function ScheduleEventShutdown(Float afDelay)
    StartTimer(afDelay, 1164)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1164
        If IsRunning() && !IsStageDone(9992)
            SetStage(9992)
        EndIf
    ElseIf aiTimerID == 1165
        ; DefaultAliasOnLoad only fires when the spawn centre loads, which never happens
        ; for a marker the player is already standing next to.
        If IsRunning() && !IsEventResolved() && IsStageDone(110) && !IsStageDone(120)
            SetStage(120)
        EndIf
    EndIf
EndEvent

Function Fragment_Stage_0010_Item_00()
    CancelTimer(1164)
    CancelTimer(1165)
    ResetEventObjectives()
    PublishWaveCount(0)
    SetAirportRespawnEnabled(False)
    SetSirenEnabled(True)
    ; Objective 15 carries no text in FO76; it only runs the air-raid timer that sets stage 50.
    SetObjectiveDisplayed(15, True)
    SetObjectiveDisplayed(5, True, True)
    If !IsStageDone(25)
        SetStage(25)
    EndIf
EndFunction

Function Fragment_Stage_0025_Item_00()
    If IsEventResolved()
        Return
    EndIf
    SetObjectiveDisplayed(5, True, True)
EndFunction

Function Fragment_Stage_0050_Item_00()
    If FF11_Raid_AirRaidSirenMarker != None
        FF11_Raid_AirRaidSirenMarker.Disable(False)
    EndIf
    ; Objective 15 exists only to run the air-raid countdown that lands here.
    CompleteOpenObjective(15)
EndFunction

Function Fragment_Stage_0100_Item_00()
    ; StartTimer stage: B21:QuestTimer arms the 900 s activity timer from here.
    If IsEventResolved()
        Return
    EndIf
    CompleteOpenObjective(5)
    PublishWaveCount(0)
    SetObjectiveDisplayed(10, True, True)
    ShowIntroMessage()
    If !IsStageDone(110)
        SetStage(110)
    EndIf
EndFunction

Function Fragment_Stage_0110_Item_00()
    If IsEventResolved()
        Return
    EndIf
    StartTimer(3.0, 1165)
EndFunction

Function Fragment_Stage_0120_Item_00()
    CancelTimer(1165)
    StartScorchedWave("Wave01", 1, 18, 0)
EndFunction

Function Fragment_Stage_0125_Item_00()
    BeginWaveBreak(18, 20)
EndFunction

Function Fragment_Stage_0150_Item_00()
    StartScorchedWave("Wave02", 2, 25, 20)
EndFunction

Function Fragment_Stage_0175_Item_00()
    BeginWaveBreak(25, 30)
EndFunction

Function Fragment_Stage_0200_Item_00()
    StartScorchedWave("Wave03", 3, 35, 30)
EndFunction

Function Fragment_Stage_0225_Item_00()
    BeginWaveBreak(35, 40)
EndFunction

Function Fragment_Stage_0250_Item_00()
    StartScorchedWave("Wave04", 4, 45, 40)
EndFunction

Function Fragment_Stage_0275_Item_00()
    BeginWaveBreak(45, 50)
EndFunction

Function Fragment_Stage_0300_Item_00()
    StartScorchedWave("Wave05", 5, 55, 50)
EndFunction

Function Fragment_Stage_0325_Item_00()
    ; FO76 marks break 5 unused; wave 5 ends the fight through its own StageToSetAtEnd.
    BeginWaveBreak(55, 60)
EndFunction

Function Fragment_Stage_0350_Item_00()
    If IsEventResolved()
        Return
    EndIf
    CompleteOpenObjective(60)
    DefaultQuestEncounterWaveScript waveScript = GetWaveScript()
    If waveScript != None && waveScript.FindEncounterWaveIndex("Wave06") >= 0
        StartScorchedWave("Wave06", 6, 65, 60)
        Return
    EndIf
    ; No sixth wave shipped, so the landing zone is clear.
    If !IsStageDone(500)
        SetStage(500)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    If IsEventResolved()
        Return
    EndIf
    CancelTimer(1165)
    StopScorchedWaves()
    CompleteFightObjectives()
    CompleteOpenObjective(10)
    SetObjectiveDisplayed(70, True, True)
    FF11_Raid_AirDropScript airDrop = GetAirDropScript()
    If airDrop != None
        airDrop.StartAirDrop()
    EndIf
EndFunction

Function Fragment_Stage_1300_Item_00()
    ; Late-join cutoff: DefaultEventQuest reads the stage itself, so the fragment only
    ; repairs the counter and objectives for a player who joined before the cutoff.
    If IsEventResolved()
        Return
    EndIf
    SetObjectiveDisplayed(10, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    StopEventTimer()
    CancelTimer(1165)
    StopScorchedWaves()
    CompleteFightObjectives()
    CompleteOpenObjective(10)
    CompleteOpenObjective(70)
    SetSirenEnabled(False)
    SetAirportRespawnEnabled(True)
    ; The supply drop stays lootable for FF11_Raid_CargoLootTimer seconds.
    FF11_Raid_QuestScript raidScript = GetRaidScript()
    If raidScript != None
        raidScript.StartCargoLootWindow()
    Else
        ScheduleEventShutdown(60.0)
    EndIf
EndFunction

Function Fragment_Stage_9991_Item_00()
    If IsStageDone(9000)
        Return
    EndIf
    StopEventTimer()
    CancelTimer(1165)
    StopScorchedWaves()
    FailEventObjectives()
    BroadcastEventTopic(FF11_Raid_EBSTopic3)
    SetSirenEnabled(False)
    SetAirportRespawnEnabled(True)
    ScheduleEventShutdown(20.0)
EndFunction

Function Fragment_Stage_9992_Item_00()
    CancelTimer(1164)
    CancelTimer(1165)
    StopEventTimer()
    StopScorchedWaves()
    SetSirenEnabled(False)
    SetAirportRespawnEnabled(True)
    If IsRunning()
        Stop()
    EndIf
EndFunction

Function Fragment_Stage_9993_Item_00()
    If IsStageDone(9000) || IsStageDone(9992)
        Return
    EndIf
    ; Losing the Cargobot aborts the supply drop.
    If !IsStageDone(9991)
        SetStage(9991)
    EndIf
EndFunction
