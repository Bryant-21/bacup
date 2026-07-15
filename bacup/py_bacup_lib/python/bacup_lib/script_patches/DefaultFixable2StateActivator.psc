; Bridge the FO76 helper names onto FO4's Default2StateActivator API.

Bool Function IsActivatorBroken()
    Return GetState() == "Open" || GetState() == "opening" || GetState() == "startsopen"
EndFunction

Function SetActivatorOpen(Bool shouldOpen)
    SetOpenNoWait(shouldOpen)
EndFunction

Function SetActivatorOpenAndWait(Bool shouldOpen)
    SetOpen(shouldOpen)
EndFunction

Function SetActivatorBroken(Bool isBroken, Bool shouldChangeStateSilently)
    If shouldChangeStateSilently
        If isBroken
            GoToState("Open")
        Else
            GoToState("Closed")
        EndIf
    Else
        SetActivatorOpen(isBroken)
    EndIf
EndFunction

Function SetActivatorBrokenAndWait(Bool isBroken, Bool shouldChangeStateSilently)
    If shouldChangeStateSilently
        If isBroken
            GoToState("Open")
        Else
            GoToState("Closed")
        EndIf
    Else
        SetActivatorOpenAndWait(isBroken)
    EndIf
EndFunction
