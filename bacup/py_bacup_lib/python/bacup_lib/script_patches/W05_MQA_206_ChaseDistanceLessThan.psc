Event OnAliasInit()
    Quest owningQuest = GetOwningQuest()
    ObjectReference distanceRef = GetReference()
    ObjectReference targetRef = None

    If TargetAlias != None
        targetRef = TargetAlias.GetReference()
    EndIf
    If owningQuest == None || distanceRef == None || targetRef == None
        Return
    EndIf

    If StageToRegister < 0 || owningQuest.IsStageDone(StageToRegister)
        CachedDistanceRef = targetRef
        RegisterForDistanceLessThanEvent(distanceRef, targetRef, fTargetDistance)
    Else
        RegisterForRemoteEvent(owningQuest, "OnStageSet")
    EndIf
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || akSender != owningQuest || auiStageID != StageToRegister
        Return
    EndIf

    ObjectReference distanceRef = GetReference()
    ObjectReference targetRef = None
    If TargetAlias != None
        targetRef = TargetAlias.GetReference()
    EndIf
    If distanceRef == None || targetRef == None
        Return
    EndIf

    CachedDistanceRef = targetRef
    RegisterForDistanceLessThanEvent(distanceRef, targetRef, fTargetDistance)
EndEvent

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    Quest owningQuest = GetOwningQuest()
    ObjectReference distanceRef = GetReference()
    Actor targetActor = CachedDistanceRef as Actor

    If owningQuest == None || distanceRef == None || targetActor == None || W05_MQA_206P_MustDealWithJohnnyAV == None
        Return
    EndIf
    If akObj1 != distanceRef || akObj2 != CachedDistanceRef
        Return
    EndIf
    If !owningQuest.IsStageDone(StageToSet) && targetActor.GetValue(W05_MQA_206P_MustDealWithJohnnyAV) == 0.0
        owningQuest.SetStage(StageToSet)
    EndIf
EndEvent
