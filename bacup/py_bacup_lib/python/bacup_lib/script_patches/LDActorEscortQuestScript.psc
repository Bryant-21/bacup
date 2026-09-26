Actor Function GetEscortActor()
    If EscortActor == None
        Return None
    EndIf
    Return EscortActor.GetActorReference()
EndFunction

Int Function GetEscortBehavior()
    Actor escort = GetEscortActor()
    If escort == None || LDActorEscortBehaviorValue == None
        Return BEHAVIOR_CUSTOM
    EndIf
    Return escort.GetValue(LDActorEscortBehaviorValue) as Int
EndFunction

Function SetEscortBehavior(Int aiBehavior)
    Actor escort = GetEscortActor()
    If escort == None || LDActorEscortBehaviorValue == None || lock_Behavior
        Return
    EndIf
    lock_Behavior = True
    escort.SetValue(LDActorEscortBehaviorValue, aiBehavior as Float)
    escort.EvaluatePackage()
    lock_Behavior = False
EndFunction

Function LinkEscortTo(ObjectReference akTarget)
    Actor escort = GetEscortActor()
    If escort == None || akTarget == None || LDActorEscortBehaviorLinkKeyword == None
        Return
    EndIf
    escort.SetLinkedRef(akTarget, LDActorEscortBehaviorLinkKeyword)
EndFunction

ObjectReference Function GetEscortTarget()
    Actor escort = GetEscortActor()
    If escort == None || LDActorEscortBehaviorLinkKeyword == None
        Return None
    EndIf
    Return escort.GetLinkedRef(LDActorEscortBehaviorLinkKeyword)
EndFunction

ObjectReference Function GetPatrolDestination()
    ObjectReference node = GetEscortTarget()
    Int hops = 0
    While node != None && hops < 32
        ObjectReference nextNode = node.GetLinkedRef()
        If nextNode == None || nextNode == node
            Return node
        EndIf
        node = nextNode
        hops += 1
    EndWhile
    Return node
EndFunction

Function WatchEscortPatrol()
    CancelTimer(43101)
    If stageToSetWhenCurrentPatrolCompleted <= 0 || !IsRunning()
        Return
    EndIf
    StartTimer(5.0, 43101)
EndFunction

Function EvaluateEscortPatrol()
    Int completedStage = stageToSetWhenCurrentPatrolCompleted
    If completedStage <= 0 || !IsRunning() || IsStageDone(completedStage)
        stageToSetWhenCurrentPatrolCompleted = 0
        Return
    EndIf
    Actor escort = GetEscortActor()
    If escort == None || escort.IsDead()
        Return
    EndIf
    ; FO76 advanced each leg from placed trigger boxes whose quest binding does not survive the
    ; port, so a leg counts as walked once the escort reaches the last node of its patrol chain.
    ObjectReference destination = GetPatrolDestination()
    If destination != None && escort.GetDistance(destination) <= 400.0
        stageToSetWhenCurrentPatrolCompleted = 0
        SetStage(completedStage)
        Return
    EndIf
    WatchEscortPatrol()
EndFunction

Function StartEscortPatrol(ObjectReference akPatrolStart, Int aiStageWhenCompleted)
    If GetEscortActor() == None
        Return
    EndIf
    stageToSetWhenCurrentPatrolCompleted = aiStageWhenCompleted
    LinkEscortTo(akPatrolStart)
    SetEscortBehavior(BEHAVIOR_PATROL)
    WatchEscortPatrol()
EndFunction

Function HoldEscortAt(ObjectReference akWaitMarker)
    If GetEscortActor() == None
        Return
    EndIf
    stageToSetWhenCurrentPatrolCompleted = 0
    CancelTimer(43101)
    LinkEscortTo(akWaitMarker)
    SetEscortBehavior(BEHAVIOR_WAIT)
EndFunction

Function StopEscortPatrol()
    stageToSetWhenCurrentPatrolCompleted = 0
    CancelTimer(43101)
    SetEscortBehavior(BEHAVIOR_CUSTOM)
EndFunction

Function WatchEscortActor()
    Actor escort = GetEscortActor()
    If escort != None
        RegisterForRemoteEvent(escort, "OnDeath")
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
EndFunction

Event OnQuestInit()
    WatchEscortActor()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    ; Papyrus timers do not always survive a reload, and a leg can be pending for minutes.
    WatchEscortPatrol()
EndEvent

Event OnQuestShutdown()
    CancelTimer(43101)
    stageToSetWhenCurrentPatrolCompleted = 0
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 43101
        EvaluateEscortPatrol()
    EndIf
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    If akSender != GetEscortActor() || StageToSetOnEscortActorDeath <= 0
        Return
    EndIf
    stageToSetWhenCurrentPatrolCompleted = 0
    CancelTimer(43101)
    If IsRunning() && !IsStageDone(StageToSetOnEscortActorDeath)
        SetStage(StageToSetOnEscortActorDeath)
    EndIf
EndEvent
