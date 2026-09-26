Actor Function GetPatrolGuard()
    If guard != None
        Return guard
    EndIf
    If TW043Guard != None
        guard = TW043Guard.GetActorReference()
    EndIf
    If guard == None
        guard = GetEscortActor()
    EndIf
    Return guard
EndFunction

Bool Function HasPatrolEnded()
    Return hasPatrolEnded || IsStageDone(230) || IsStageDone(CONST_StopStage)
EndFunction

Function SayPatrolTopic(Topic akTopic, ReferenceAlias akSpeakerAlias = None)
    If akTopic == None
        Return
    EndIf
    Actor speaker
    If akSpeakerAlias != None
        speaker = akSpeakerAlias.GetActorReference()
    EndIf
    If speaker == None
        speaker = GetPatrolGuard()
    EndIf
    If speaker != None
        speaker.Say(akTopic)
    EndIf
EndFunction

Function AnnouncePatrolIntro(ReferenceAlias akSpeakerAlias = None)
    SayPatrolTopic(TW043_IntroTopic, akSpeakerAlias)
EndFunction

Function AnnouncePatrolFailure(ReferenceAlias akSpeakerAlias = None)
    SayPatrolTopic(TW043_FailTopic, akSpeakerAlias)
EndFunction

Function BeginPatrol()
    If !IsRunning() || HasPatrolEnded() || IsStageDone(CONST_PatrolStartedStage)
        Return
    EndIf
    SetStage(CONST_PatrolStartedStage)
EndFunction

Function StartChosenPatrolRoute()
    ; Route 2 (yard -> solitary -> D-Wing) is the first patrol the guard ever runs; later patrols
    ; pick either route. PatrolRoute is read by the route conditions on the patrol content.
    If PatrolRoute <= 0
        PatrolRoute = 2
    Else
        PatrolRoute = Utility.RandomInt(1, 2)
    EndIf
    Int routeStage = 12
    If PatrolRoute == 1
        routeStage = 11
    EndIf
    If IsRunning() && !HasPatrolEnded() && !IsStageDone(routeStage)
        SetStage(routeStage)
    EndIf
EndFunction

Function RetireOldGuard()
    If TW043OldGuard == None
        Return
    EndIf
    ObjectReference oldGuard = TW043OldGuard.GetReference()
    Actor currentGuard = GetPatrolGuard()
    If oldGuard == None || oldGuard == currentGuard
        Return
    EndIf
    ; The pod keeps the previous patrol's Protectron; hide it so only the fresh guard deploys.
    oldGuard.Disable(False)
EndFunction

B21:QuestTimer Function EventClock()
    Quest owner = Self as Quest
    Return owner as B21:QuestTimer
EndFunction

Function StartEventClock()
    B21:QuestTimer eventClock = EventClock()
    If eventClock == None
        Return
    EndIf
    ; No TW043 stage carries StartTimer/TimerEnd, so the patrol's own 45 minute global is the clock.
    Float seconds = -1.0
    If TW043_EventTimer != None && TW043_EventTimer.GetValue() > 0.0
        seconds = TW043_EventTimer.GetValue()
    EndIf
    eventClock.StartQuestTimer(seconds)
EndFunction

Function StopEventClock()
    B21:QuestTimer eventClock = EventClock()
    If eventClock != None
        eventClock.StopQuestTimer()
    EndIf
EndFunction

Function OpenTerminalDoor(ReferenceAlias akTerminalAlias)
    If akTerminalAlias == None || LinkTerminalSwitchDoor == None
        Return
    EndIf
    ObjectReference terminalRef = akTerminalAlias.GetReference()
    If terminalRef == None
        Return
    EndIf
    ObjectReference doorRef = terminalRef.GetLinkedRef(LinkTerminalSwitchDoor)
    If doorRef == None
        Return
    EndIf
    doorRef.Lock(False)
    doorRef.SetOpen(True)
EndFunction

Function OpenSecurityDoors()
    If TW043_IntakeSecurityGate01 != None
        TW043_IntakeSecurityGate01.SetOpen(True)
    EndIf
    If TW043_IntakeSecurityGate02 != None
        TW043_IntakeSecurityGate02.SetOpen(True)
    EndIf
    OpenTerminalDoor(TW043_IntakeSecurityTerminal01)
    OpenTerminalDoor(TW043_IntakeSecurityTerminal02)
    HoldEscortAt(TW043_100_SecurityDoor1Marker)
EndFunction

Function FlushStageWhenLoaded()
    CancelTimer(43102)
    Int pending = stageToSetWhenLoaded
    If pending <= 0 || !IsRunning()
        Return
    EndIf
    If HasPatrolEnded() || IsStageDone(pending)
        stageToSetWhenLoaded = -1
        Return
    EndIf
    Actor guardRef = GetPatrolGuard()
    If guardRef != None && guardRef.Is3DLoaded()
        stageToSetWhenLoaded = -1
        SetStage(pending)
        Return
    EndIf
    StartTimer(2.0, 43102)
EndFunction

Function SetStageWhenGuardLoaded(Int aiStage)
    stageToSetWhenLoaded = aiStage
    FlushStageWhenLoaded()
EndFunction

Function EndPatrol()
    hasPatrolEnded = True
    stageToSetWhenLoaded = -1
    CancelTimer(43102)
    StopEscortPatrol()
    StopEventClock()
EndFunction

Event OnQuestInit()
    Parent.OnQuestInit()
    guard = None
    hasPatrolEnded = False
    stageToSetWhenLoaded = -1
    B21:QuestTimer eventClock = EventClock()
    If eventClock != None
        RegisterForCustomEvent(eventClock, "QuestTimerEnded")
    EndIf
    Actor guardRef = GetPatrolGuard()
    If guardRef != None
        RegisterForRemoteEvent(guardRef, "OnEnterBleedout")
    EndIf
EndEvent

Event OnQuestShutdown()
    Parent.OnQuestShutdown()
    CancelTimer(43102)
    stageToSetWhenLoaded = -1
    StopEventClock()
    guard = None
EndEvent

Event OnTimer(Int aiTimerID)
    Parent.OnTimer(aiTimerID)
    If aiTimerID == 43102
        FlushStageWhenLoaded()
    EndIf
EndEvent

Event B21:QuestTimer.QuestTimerEnded(B21:QuestTimer akSender, Var[] akArgs)
    If !IsRunning() || HasPatrolEnded()
        Return
    EndIf
    SayPatrolTopic(TW043_FailTimeoutTopic)
    SetStage(CONST_StopStage)
EndEvent

Event Actor.OnEnterBleedout(Actor akSender)
    If !IsRunning() || HasPatrolEnded() || akSender != GetPatrolGuard()
        Return
    EndIf
    ; FO4 has no robot-repair interaction, so the repair objective only tracks the downed guard.
    SetObjectiveDisplayed(CONST_RepairTheGuardTimedObjective, True, True)
EndEvent
