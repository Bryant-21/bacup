Event OnInit()
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None
        RegisterForRemoteEvent(owningQuest, "OnStageSet")
        If owningQuest.GetStage() >= PreReqStage
            UnlockUpstairsDoor()
        EndIf
    EndIf
EndEvent

Event Quest.OnStageSet(Quest akSender, int auiStageID, int auiItemID)
    If auiStageID >= PreReqStage
        UnlockUpstairsDoor()
    EndIf
EndEvent

Function UnlockUpstairsDoor()
    ObjectReference doorRef = GetReference()
    If doorRef != None && doorRef.IsLocked()
        doorRef.Unlock()
    EndIf
EndFunction
