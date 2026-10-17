Event OnDestructionStageChanged(ObjectReference akSenderRef, Int aiOldStage, Int aiCurrentStage)
    If akSenderRef == None || !akSenderRef.IsDestroyed() || !CanContribute()
        Return
    EndIf
    If TargetKeyword != None && !akSenderRef.HasKeyword(TargetKeyword)
        Return
    EndIf
    If AlreadyCounted == None
        AlreadyCounted = new ObjectReference[0]
    EndIf
    If AlreadyCounted.Find(akSenderRef) >= 0
        Return
    EndIf
    AlreadyCounted.Add(akSenderRef)
    ContributeProgress(1.0, akSenderRef)
EndEvent
