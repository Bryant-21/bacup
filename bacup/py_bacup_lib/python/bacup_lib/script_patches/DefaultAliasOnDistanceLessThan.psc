Event OnAliasInit()
    OwningQuest = GetOwningQuest()
    If OwningQuest != None
        RegisterForRemoteEvent(OwningQuest, "OnStageSet")
    EndIf
    RegisterDistanceEvent()
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If akSender == OwningQuest && (StageToRegister < 0 || OwningQuest.IsStageDone(StageToRegister))
        RegisterDistanceEvent()
    EndIf
EndEvent

Function RegisterDistanceEvent()
    If OwningQuest == None
        OwningQuest = GetOwningQuest()
    EndIf
    CachedDistanceRef = GetReference()
    If TargetAlias != None
        ActivePlayer = TargetAlias.GetReference() as Actor
    EndIf

    If OwningQuest != None && CachedDistanceRef != None && TargetAlias != None && fTargetDistance > 0.0
        If !OwningQuest.IsStageDone(StageToSet) && (StageToRegister < 0 || OwningQuest.IsStageDone(StageToRegister))
            RegisterForDistanceLessThanEvent(Self, TargetAlias, fTargetDistance)
        EndIf
    EndIf
EndFunction

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    If OwningQuest == None || OwningQuest.IsStageDone(StageToSet)
        Return
    EndIf
    If StageToRegister >= 0 && !OwningQuest.IsStageDone(StageToRegister)
        Return
    EndIf
    OwningQuest.SetStage(StageToSet)
EndEvent
