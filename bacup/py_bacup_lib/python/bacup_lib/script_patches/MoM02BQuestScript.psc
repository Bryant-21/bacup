Event OnQuestInit()
    ReconcileRuntimeRegistrations()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        ReconcileRuntimeRegistrations()
    EndIf
EndEvent

Event OnQuestShutdown()
    ClearRuntimeRegistrations()
EndEvent

Function ClearRuntimeRegistrations()
    Actor player = Game.GetPlayer()
    If player != None
        UnregisterForRemoteEvent(player, "OnKill")
        UnregisterForRemoteEvent(player, "OnPlayerLoadGame")
        UnregisterForRemoteEvent(player, "OnPlayerModArmorWeapon")
    EndIf
EndFunction

Function ReconcileRuntimeRegistrations()
    ClearRuntimeRegistrations()
    If !IsRunning() || IsCompleted()
        Return
    EndIf

    Actor player = Game.GetPlayer()
    If player != None
        RegisterForRemoteEvent(player, "OnKill")
        RegisterForRemoteEvent(player, "OnPlayerLoadGame")
        RegisterForRemoteEvent(player, "OnPlayerModArmorWeapon")
    EndIf
EndFunction

Event Actor.OnPlayerModArmorWeapon(Actor akSender, Form akBaseObject, ObjectMod akModBaseObject)
    If akSender == Game.GetPlayer() && IsRunning() && !IsCompleted() && akBaseObject == MoM02BHistoricSword && akModBaseObject == MoM02BSwingAnalyzerMod && !IsStageDone(60)
        SetStage(60)
    EndIf
EndEvent

Event Actor.OnKill(Actor akSender, Actor akVictim)
    If akSender != Game.GetPlayer() || akVictim == None || !IsRunning() || !IsStageDone(CONST_CHECKPOINT_MoM02B_KillCreatures) || IsStageDone(CONST_CHECKPOINT_MoM02B_FabricateBlade)
        Return
    EndIf
    If akSender.GetEquippedWeapon() != MoM02BHistoricSword || lock_KillTracker
        Return
    EndIf

    lock_KillTracker = True
    TrackDistinctCreatureType(akVictim)
    lock_KillTracker = False
EndEvent

Function TrackDistinctCreatureType(Actor akVictim)
    If MoM02BActorList == None || MoM02BKillObjectives == None || killObjectiveCurrent >= CONST_KillObjectiveTotal || killObjectiveCurrent >= MoM02BKillObjectives.Length
        Return
    EndIf

    Int actorTypeIndex = 0
    While actorTypeIndex < MoM02BActorList.Length
        Keyword actorTypeKeyword = MoM02BActorList[actorTypeIndex].ActorTypeKeyword
        Location actorTypeName = MoM02BActorList[actorTypeIndex].ActorTypeName
        If actorTypeKeyword != None && actorTypeName != None && akVictim.HasKeyword(actorTypeKeyword) && !HasKilledActorType(actorTypeName)
            RecordKilledActorType(actorTypeName)
            MoM02BKillObjectiveData killObjective = MoM02BKillObjectives[killObjectiveCurrent]
            If killObjective.ObjectiveAlias != None
                killObjective.ObjectiveAlias.ForceLocationTo(actorTypeName)
            EndIf
            SetObjectiveDisplayed(killObjective.objectiveIndex)
            SetObjectiveCompleted(killObjective.objectiveIndex)
            killObjectiveCurrent += 1
            If killObjectiveCurrent >= CONST_KillObjectiveTotal && !IsStageDone(CONST_CHECKPOINT_MoM02B_FabricateBlade)
                SetStage(CONST_CHECKPOINT_MoM02B_FabricateBlade)
            Else
                SetObjectiveDisplayed(70, True, True)
            EndIf
            Return
        EndIf
        actorTypeIndex += 1
    EndWhile
EndFunction

Bool Function HasKilledActorType(Location akActorTypeName)
    If killedActorsList == None
        Return False
    EndIf

    Int index = 0
    While index < killedActorsList.Length
        If killedActorsList[index] == akActorTypeName
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

Function RecordKilledActorType(Location akActorTypeName)
    If killedActorsList == None
        killedActorsList = new Location[1]
        killedActorsList[0] = akActorTypeName
    Else
        killedActorsList.Add(akActorTypeName)
    EndIf
EndFunction
