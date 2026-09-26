Event OnAliasInit()
    Quest owningQuest = GetOwningQuest()
    ObjectReference bloomRef = GetReference()
    If owningQuest != None && bloomRef != None
        If owningQuest.IsStageDone(600)
            bloomRef.EnableNoWait()
        Else
            bloomRef.DisableNoWait()
        EndIf
    EndIf
EndEvent
