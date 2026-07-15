Event OnUnload(ObjectReference akSenderRef)
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || owningQuest.GetStage() >= iWatchForUnloadStage
        If !akSenderRef.IsDisabled()
            akSenderRef.DisableNoWait()
        EndIf
        akSenderRef.Delete()
        RemoveRef(akSenderRef)
    EndIf
EndEvent
