DefaultQuestEncounterWaveScript Function GetWaveScript()
    Quest owner = Self as Quest
    Return owner as DefaultQuestEncounterWaveScript
EndFunction

B21:QuestTimer Function GetEventTimer()
    Quest owner = Self as Quest
    Return owner as B21:QuestTimer
EndFunction

Bool Function IsEventResolved()
    Return IsStageDone(1000) || IsStageDone(1500) || IsStageDone(1600)
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
EndFunction

Actor Function GetVertibot()
    If Alias_Vertibird == None
        Return None
    EndIf
    Return Alias_Vertibird.GetActorReference()
EndFunction

Function StartCritterWave(String asWaveID)
    If IsEventResolved()
        Return
    EndIf
    DefaultQuestEncounterWaveScript waveScript = GetWaveScript()
    If waveScript != None
        waveScript.StartEncounterWaveByID(asWaveID)
    EndIf
EndFunction

Function StopCritterWaves()
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
    ; The wreck stays lootable for the grace period; stage 1600 is FO76's shutdown stage.
    StartTimer(afDelay, 2918)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 2918 && IsRunning() && !IsStageDone(1600)
        SetStage(1600)
    EndIf
EndEvent

Function Fragment_Stage_0010_Item_00()
    CancelTimer(2918)
    ResetEventObjectives()
    Actor vertibot = GetVertibot()
    If vertibot != None && vertibot.IsDisabled()
        vertibot.Enable(False)
    EndIf
    SetObjectiveDisplayed(10, True, True)
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    ; StartTimer stage: B21:QuestTimer arms the 900 s activity timer from here.
    If IsEventResolved()
        Return
    EndIf
    Actor vertibot = GetVertibot()
    If vertibot != None && vertibot.IsDisabled()
        vertibot.Enable(False)
    EndIf
    SetObjectiveDisplayed(10, True, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
    StartCritterWave("Wave1")
EndFunction

Function Fragment_Stage_0300_Item_00()
    StartCritterWave("Wave2")
EndFunction

Function Fragment_Stage_0400_Item_00()
    StartCritterWave("Wave3")
EndFunction

Function Fragment_Stage_1000_Item_00()
    StopEventTimer()
    If IsObjectiveDisplayed(10) && !IsObjectiveFailed(10)
        SetObjectiveCompleted(10, True)
    EndIf
    StopCritterWaves()
    ScheduleEventShutdown(60.0)
EndFunction

Function Fragment_Stage_1500_Item_00()
    If IsStageDone(1000)
        Return
    EndIf
    StopEventTimer()
    If IsObjectiveDisplayed(10) && !IsObjectiveCompleted(10)
        SetObjectiveFailed(10, True)
    EndIf
    StopCritterWaves()
    ; The survey Vertibot leaves the area; the temp alias reference is released on shutdown.
    Actor vertibot = GetVertibot()
    If vertibot != None && !vertibot.IsDead()
        vertibot.Disable(True)
    EndIf
    ScheduleEventShutdown(20.0)
EndFunction

Function Fragment_Stage_1600_Item_00()
    CancelTimer(2918)
    StopEventTimer()
    StopCritterWaves()
    If IsRunning()
        Stop()
    EndIf
EndFunction
