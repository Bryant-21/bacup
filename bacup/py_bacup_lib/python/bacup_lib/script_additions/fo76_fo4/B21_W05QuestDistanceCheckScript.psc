Scriptname B21_W05QuestDistanceCheckScript Extends DefaultQuestDistanceCheckScript

Event OnQuestInit()
    RegisterPlayerLoad()
    RefreshDistanceWatches()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    RefreshDistanceWatches()
EndEvent

Function RegisterPlayerLoad()
    Actor playerRef = Game.GetPlayer()
    If IsRunning() && playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
EndFunction

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        RefreshDistanceWatches()
    EndIf
EndEvent

Bool Function DistanceStageIsReady(DefaultQuestDistanceCheckScript:DistanceCheckStage akCheck)
    If !IsRunning() || akCheck == None || akCheck.DistanceCheckAlias == None
        Return False
    EndIf
    If akCheck.StageToSet < 0 || IsStageDone(akCheck.StageToSet)
        Return False
    EndIf
    If akCheck.PrereqStage >= 0 && !IsStageDone(akCheck.PrereqStage)
        Return False
    EndIf
    Return akCheck.TurnOffStage < 0 || GetStage() < akCheck.TurnOffStage
EndFunction

Function RefreshDistanceWatches()
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || DistanceCheckStages == None
        Return
    EndIf
    Int i = 0
    While i < DistanceCheckStages.Length
        DefaultQuestDistanceCheckScript:DistanceCheckStage check = DistanceCheckStages[i]
        If check != None && check.DistanceCheckAlias != None
            UnregisterForDistanceEvents(playerRef, check.DistanceCheckAlias)
            If DistanceStageIsReady(check)
                If check.DistanceLessThan
                    RegisterForDistanceLessThanEvent(playerRef, check.DistanceCheckAlias, check.Distance)
                Else
                    RegisterForDistanceGreaterThanEvent(playerRef, check.DistanceCheckAlias, check.Distance)
                EndIf
            EndIf
        EndIf
        i += 1
    EndWhile
EndFunction

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    If akObj1 == Game.GetPlayer()
        CheckDistanceStages(True, akObj2)
    ElseIf akObj2 == Game.GetPlayer()
        CheckDistanceStages(True, akObj1)
    EndIf
EndEvent

Event OnDistanceGreaterThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    If akObj1 == Game.GetPlayer()
        CheckDistanceStages(False, akObj2)
    ElseIf akObj2 == Game.GetPlayer()
        CheckDistanceStages(False, akObj1)
    EndIf
EndEvent

Function CheckDistanceStages(Bool bDistanceLessThan, ObjectReference theRef)
    If theRef == None || DistanceCheckStages == None
        Return
    EndIf
    Int i = 0
    While i < DistanceCheckStages.Length
        DefaultQuestDistanceCheckScript:DistanceCheckStage check = DistanceCheckStages[i]
        If DistanceStageIsReady(check)
            If check.DistanceLessThan == bDistanceLessThan && check.DistanceCheckAlias.GetReference() == theRef
                SetStage(check.StageToSet)
            EndIf
        EndIf
        i += 1
    EndWhile
EndFunction

Event OnQuestShutdown()
    UnregisterForAllRemoteEvents()
EndEvent
