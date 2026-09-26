Event OnInit()
    UnregisterForAllRemoteEvents()
EndEvent

Event Quest.OnStageSet(Quest akSender, int auiStageID, int auiItemID)
    UnregisterForRemoteEvent(akSender, "OnStageSet")
EndEvent

Event OnPackageEnd(ObjectReference akSenderRef, Package akOldPackage)
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsRunning() || !owningQuest.IsStageDone(1260) || !owningQuest.IsStageDone(1300)
        Return
    EndIf
    If W05_MQ_004P_Crane_RadicalWalkTowardExit == None || akOldPackage != W05_MQ_004P_Crane_RadicalWalkTowardExit || akSenderRef == None || Find(akSenderRef) < 0 || MoveToMarker == None
        Return
    EndIf
    Actor crewActor = akSenderRef as Actor
    ObjectReference exitMarker = MoveToMarker.GetReference()
    If crewActor == None || crewActor.IsDead() || crewActor.IsInCombat() || exitMarker == None
        Return
    EndIf
    crewActor.MoveTo(exitMarker)
EndEvent
