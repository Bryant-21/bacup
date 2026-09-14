Event OnTriggerEnter(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    ObjectReference triggerRef = GetRef()
    If triggerRef == None || CurrentTarget == None
        Return
    EndIf

    ObjectReference linkedTarget = triggerRef.GetLinkedRef()
    If linkedTarget == None
        Return
    EndIf

    If CurrentTarget.GetRef() != linkedTarget
        CurrentTarget.ForceRefTo(linkedTarget)
    EndIf

    If PreviousTriggerVolumes == None
        Return
    EndIf

    Int index = 0
    While index < PreviousTriggerVolumes.Length
        ReferenceAlias previousTrigger = PreviousTriggerVolumes[index]
        If previousTrigger != None
            ObjectReference previousTriggerRef = previousTrigger.GetRef()
            If previousTriggerRef != None && !previousTriggerRef.IsDisabled()
                previousTrigger.TryToDisableNoWait()
            EndIf
        EndIf
        index += 1
    EndWhile
EndEvent
