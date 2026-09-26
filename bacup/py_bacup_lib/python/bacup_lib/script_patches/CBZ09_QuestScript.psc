Actor Function RobotActor()
    If Robot == None
        Return None
    EndIf
    Return Robot.GetActorReference()
EndFunction

DefaultQuestEncounterWaveScript Function EventWaves()
    If selfAsEncounterWaveScript == None
        Quest owner = Self as Quest
        selfAsEncounterWaveScript = owner as DefaultQuestEncounterWaveScript
    EndIf
    Return selfAsEncounterWaveScript
EndFunction

Bool Function IsEventResolved()
    If IsStopping() || IsStopped() || IsCompleted()
        Return True
    EndIf
    Return IsStageDone(SuccessQuestStage) || IsStageDone(RobotDiesFailQuestStage) || IsStageDone(9991)
EndFunction

Function WatchRobot(Bool abWatch)
    Actor robotActor = RobotActor()
    If robotActor == None
        ; The robot alias is created at the event's center marker, so it can fill after init.
        If abWatch && IsRunning() && !IsEventResolved()
            StartTimer(5.0, 52084)
        EndIf
        Return
    EndIf
    If abWatch
        RegisterForRemoteEvent(robotActor, "OnDeath")
    Else
        UnregisterForRemoteEvent(robotActor, "OnDeath")
    EndIf
EndFunction

Function StartInitialScene()
    If CBZ09_Census_InitialScene != None && !CBZ09_Census_InitialScene.IsPlaying()
        CBZ09_Census_InitialScene.Start()
    EndIf
    ; The converted scene has no phase fragments, so the quest has to advance itself
    ; when the conversation ends or cannot play at all.
    StartTimer(5.0, 52083)
    StartTimer(40.0, 52085)
EndFunction

Function StartDefendPhase()
    If IsEventResolved()
        Return
    EndIf
    DefaultQuestEncounterWaveScript waves = EventWaves()
    If waves != None
        waves.StartEncounterWaveByID("Endless")
    EndIf

    Float defendSeconds = DefendObjectiveTime
    If defendSeconds < 10.0
        defendSeconds = 10.0
    EndIf
    Float stopWavesAfter = defendSeconds * DefendObjectiveTimePercentageToStopWaves
    If stopWavesAfter < 1.0 || stopWavesAfter > defendSeconds
        stopWavesAfter = defendSeconds
    EndIf
    CancelTimer(52081)
    CancelTimer(52082)
    StartTimer(stopWavesAfter, 52082)
    StartTimer(defendSeconds, 52081)
EndFunction

Function StopDefendWaves(Bool abRemoveActors)
    DefaultQuestEncounterWaveScript waves = EventWaves()
    If waves != None
        waves.StopAllEncounterWaves(abRemoveActors)
    EndIf
EndFunction

Function SayShutdownTopic()
    Actor robotActor = RobotActor()
    If robotActor != None && CBZ09_Census_ShutdownTopic != None
        robotActor.Say(CBZ09_Census_ShutdownTopic)
    EndIf
EndFunction

Event OnQuestInit()
    ActivePlayerCount = 1
    EventWaves()
    WatchRobot(True)
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 100
        StartInitialScene()
    ElseIf auiStageID == DefendObjective
        StartDefendPhase()
    EndIf
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    If akSender != RobotActor() || IsEventResolved()
        Return
    EndIf
    CancelTimer(52081)
    CancelTimer(52082)
    If RobotDiesFailQuestStage >= 0
        SetStage(RobotDiesFailQuestStage)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 52081
        If !IsEventResolved() && SuccessQuestStage >= 0
            SetStage(SuccessQuestStage)
        EndIf
    ElseIf aiTimerID == 52082
        StopDefendWaves(False)
    ElseIf aiTimerID == 52083
        If IsEventResolved() || IsStageDone(200)
            Return
        EndIf
        If CBZ09_Census_InitialScene != None && CBZ09_Census_InitialScene.IsPlaying()
            StartTimer(5.0, 52083)
            Return
        EndIf
        SetStage(200)
    ElseIf aiTimerID == 52085
        If !IsEventResolved() && !IsStageDone(200)
            SetStage(200)
        EndIf
    ElseIf aiTimerID == 52084
        WatchRobot(True)
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(52081)
    CancelTimer(52082)
    CancelTimer(52083)
    CancelTimer(52084)
    CancelTimer(52085)
    WatchRobot(False)
    If CBZ09_Census_InitialScene != None && CBZ09_Census_InitialScene.IsPlaying()
        CBZ09_Census_InitialScene.Stop()
    EndIf
    StopDefendWaves(True)
EndEvent
