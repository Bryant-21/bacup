Event OnActivate(ObjectReference akActionRef)
    If enableOnActivationKeyword != None
        ObjectReference enableTarget = GetLinkedRef(enableOnActivationKeyword)
        If enableTarget != None
            enableTarget.EnableNoWait()
        EndIf
    EndIf

    If disableOnActivationKeyword != None
        ObjectReference disableTarget = GetLinkedRef(disableOnActivationKeyword)
        If disableTarget != None
            disableTarget.DisableNoWait()
        EndIf
    EndIf

    isOn = True
EndEvent
