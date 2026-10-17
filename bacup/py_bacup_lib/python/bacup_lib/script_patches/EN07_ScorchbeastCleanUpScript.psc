; Fissure scorchbeasts that have stayed unloaded for iCleanupTimerLength seconds are removed from the collection and
; deleted, so a fissure that the player leaves does not keep its spawns around.
Event OnUnload(ObjectReference akSenderRef)
    StartTimer(iCleanupTimerLength as Float, iCleanUpTimerID)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != iCleanUpTimerID
        Return
    EndIf
    Int index = GetCount() - 1
    While index >= 0
        ObjectReference scorchbeast = GetAt(index)
        If scorchbeast != None && !scorchbeast.Is3DLoaded()
            RemoveRef(scorchbeast)
            scorchbeast.Disable()
            scorchbeast.Delete()
        EndIf
        index -= 1
    EndWhile
EndEvent
