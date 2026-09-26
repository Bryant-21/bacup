Bool Function CanUpdate()
    myQuest = GetOwningQuest()
    If myQuest == None || !myQuest.IsRunning() || myQuest.IsCompleted()
        Return False
    EndIf
    If PrereqStage >= 0 && !myQuest.IsStageDone(PrereqStage)
        Return False
    EndIf
    If TurnOffStage >= 0
        If TurnOffStage_UseGetStageDone
            If myQuest.IsStageDone(TurnOffStage)
                Return False
            EndIf
        ElseIf myQuest.GetStage() >= TurnOffStage
            Return False
        EndIf
    EndIf
    Return True
EndFunction

Function SetConfiguredStage(Int stage)
    If stage >= 0 && CanUpdate() && !myQuest.IsStageDone(stage)
        myQuest.SetStage(stage)
    EndIf
EndFunction

Function ReconcileHealth()
    If !CanUpdate()
        Return
    EndIf
    ObjectReference targetRef = GetReference()
    If targetRef == None
        Return
    EndIf
    Actor targetActor = targetRef as Actor
    If targetActor != None && targetActor.IsDead()
        SetConfiguredStage(OnDyingStage)
        Return
    EndIf
    If targetRef.IsDestroyed()
        SetConfiguredStage(OnDestroyedStage)
        Return
    EndIf
    If HealthThresholdStages != None && HealthThresholdStages.Length > 0
        ActorValue health = Game.GetFormFromFile(0x000002D4, "Fallout4.esm") as ActorValue
        If health != None
            Float percentage = targetRef.GetValuePercentage(health)
            Int index = 0
            While index < HealthThresholdStages.Length
                If percentage <= HealthThresholdStages[index].MinHealth
                    SetConfiguredStage(HealthThresholdStages[index].StageToSet)
                EndIf
                index += 1
            EndWhile
        EndIf
    EndIf
    If targetRef.Is3DLoaded() && (OnDyingStage >= 0 || HealthThresholdStages != None)
        StartTimer(HealthChangeTimerSeconds, HealthChangeTimerID)
    EndIf
EndFunction

Event OnAliasInit()
    Actor player = Game.GetPlayer()
    If player != None
        RegisterForRemoteEvent(player, "OnPlayerLoadGame")
    EndIf
    ReconcileHealth()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        ReconcileHealth()
    EndIf
EndEvent

Event OnLoad()
    ReconcileHealth()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == HealthChangeTimerID
        ReconcileHealth()
    EndIf
EndEvent

Event OnDying(Actor akKiller)
    CancelTimer(HealthChangeTimerID)
    SetConfiguredStage(OnDyingStage)
EndEvent

Event OnDeath(Actor akKiller)
    CancelTimer(HealthChangeTimerID)
    SetConfiguredStage(OnDyingStage)
EndEvent

Event OnEnterBleedout()
    SetConfiguredStage(OnenterBleedoutStage)
EndEvent

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
    If aiCurrentStage == 0 && aiOldStage > 0
        SetConfiguredStage(OnRepairedStage)
    Else
        ReconcileHealth()
    EndIf
EndEvent

Event OnUnload()
    CancelTimer(HealthChangeTimerID)
EndEvent

Event OnAliasShutdown()
    CancelTimer(HealthChangeTimerID)
    UnregisterForAllEvents()
EndEvent
