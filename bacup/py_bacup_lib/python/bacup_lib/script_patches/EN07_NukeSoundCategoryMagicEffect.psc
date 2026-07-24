Event OnEffectStart(Actor akTarget, Actor akCaster)
    If SnapshotToApply != None
        SnapshotToApply.Push(TransitionTime)
    EndIf
    If iRemovalTimerLength > 0
        StartTimer(iRemovalTimerLength as Float, iTimerID)
    EndIf
EndEvent

Event OnEffectFinish(Actor akTarget, Actor akCaster)
    CancelTimer(iTimerID)
    If !bCompleted
        If SnapshotToApply != None
            SnapshotToApply.Remove()
        EndIf
        bCompleted = True
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == iTimerID && !bCompleted
        If SnapshotToApply != None
            SnapshotToApply.Remove()
        EndIf
        bCompleted = True
    EndIf
EndEvent
