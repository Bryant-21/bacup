Event OnLoad()
    parent.OnLoad()
    If !HasKeyword(MTRZ05MiningSiteUsedKeyword)
        GoToState("open")
    EndIf
EndEvent

State open
    Event OnActivate(ObjectReference akActionRef)
        If HasKeyword(MTRZ05MiningSiteInUseKeyword)
            Return
        EndIf

        AddKeyword(MTRZ05MiningSiteInUseKeyword)
        ; SetOpen (inherited from Default2StateActivator) drives its own gotoState("busy")/
        ; gotoState("waiting") transitions internally, so the object is no longer in "open"
        ; once this returns; a later re-activation of an already-dug site falls through to
        ; the parent's own waiting-state toggle rather than being suppressed here.
        SetOpen(True)
        RemoveKeyword(MTRZ05MiningSiteInUseKeyword)
        AddKeyword(MTRZ05MiningSiteUsedKeyword)
    EndEvent
EndState
