Event OnClose(ObjectReference akSenderRef, ObjectReference akActionRef)
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || owningQuest.IsStageDone(1745)
        Return
    EndIf

    Int breakerCount = (Self as RefCollectionAlias).GetCount()
    If breakerCount == 0
        Return
    EndIf

    Int breakerIndex = 0
    While breakerIndex < breakerCount
        ObjectReference breakerRef = (Self as RefCollectionAlias).GetAt(breakerIndex)
        If breakerRef == None || breakerRef.GetOpenState() != 3
            Return
        EndIf
        breakerIndex += 1
    EndWhile

    owningQuest.SetStage(1745)
EndEvent
