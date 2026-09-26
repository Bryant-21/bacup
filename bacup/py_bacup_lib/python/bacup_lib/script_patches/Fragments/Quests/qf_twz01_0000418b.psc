Bool Function IsEventOver()
    Return IsStageDone(999) || IsStageDone(1000)
EndFunction

Function ResetEventObjective(Int aiObjective)
    SetObjectiveDisplayed(aiObjective, False)
    SetObjectiveCompleted(aiObjective, False)
    SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetEventObjectives()
    ResetEventObjective(100)
    ResetEventObjective(200)
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

Function StartEventWave(String asWaveID)
    Quest owner = Self as Quest
    DefaultQuestEncounterWaveScript waveScript = owner as DefaultQuestEncounterWaveScript
    If waveScript != None
        waveScript.StartEncounterWaveByID(asWaveID)
    EndIf
EndFunction

Function StopEventWaves()
    Quest owner = Self as Quest
    DefaultQuestEncounterWaveScript waveScript = owner as DefaultQuestEncounterWaveScript
    If waveScript != None
        waveScript.StopAllEncounterWaves(False)
    EndIf
EndFunction

Function AnnounceRobots()
    If TW006_EBSTopicSecond == None
        Return
    EndIf
    Quest owner = Self as Quest
    DefaultQuestEmergencyBroadcastScript broadcaster = owner as DefaultQuestEmergencyBroadcastScript
    If broadcaster != None
        broadcaster.SendEmergencyBroadcast(TW006_EBSTopicSecond, None)
        Return
    EndIf
    SQ_MasterScript masterScript = SQ_Master as SQ_MasterScript
    If masterScript != None && masterScript.EBSDefaultActor != None
        masterScript.EBSDefaultActor.Say(TW006_EBSTopicSecond)
    EndIf
EndFunction

Bool Function PlayerAtProtest()
    Quest owner = Self as Quest
    DefaultEventQuest eventQuest = owner as DefaultEventQuest
    If eventQuest != None
        Return eventQuest.IsPlayerParticipating()
    EndIf
    Return Alias_PlayersAllRegion != None && Alias_PlayersAllRegion.Find(Game.GetPlayer()) >= 0
EndFunction

Function WatchForArrival()
    CancelTimer(4181)
    If !IsRunning() || IsEventOver() || IsStageDone(200)
        Return
    EndIf
    ; FO76 set stage 200 from a placed trigger box at the gathering spot; that binding does not
    ; survive the port, so arrival is taken from event participation instead.
    If PlayerAtProtest()
        SetStage(200)
        Return
    EndIf
    StartTimer(5.0, 4181)
EndFunction

Function ShutdownEvent(Bool abFailed)
    CancelTimer(4181)
    StopEventWaves()
    If abFailed
        FailOpenObjective(100)
        FailOpenObjective(200)
    EndIf
    StartTimer(5.0, 4189)
EndFunction

Event OnQuestInit()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    ; Papyrus timers do not always survive a reload; the arrival poll may still owe stage 200.
    WatchForArrival()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 4181
        WatchForArrival()
    ElseIf aiTimerID == 4189
        Stop()
    EndIf
EndEvent

Function Fragment_Stage_0100_Item_00()
    ResetEventObjectives()
    SetObjectiveDisplayed(100, True, True)
    WatchForArrival()
EndFunction

Function Fragment_Stage_0200_Item_00()
    CancelTimer(4181)
    If IsEventOver()
        Return
    EndIf
    CompleteOpenObjective(100)
    SetObjectiveDisplayed(200, True, True)
    StartEventWave("First Scorched")
EndFunction

Function Fragment_Stage_0300_Item_00()
    If IsEventOver()
        Return
    EndIf
    AnnounceRobots()
    StartEventWave("Second Robots")
EndFunction

Function Fragment_Stage_0400_Item_00()
    If IsEventOver()
        Return
    EndIf
    StartEventWave("Third Scorched")
EndFunction

Function Fragment_Stage_0999_Item_00()
    ShutdownEvent(True)
EndFunction

Function Fragment_Stage_1000_Item_00()
    CompleteOpenObjective(100)
    CompleteOpenObjective(200)
    ShutdownEvent(False)
EndFunction
