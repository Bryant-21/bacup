Function Fragment_Phase_02_End()
    ObjectReference guardRef = alias_Guard.GetReference()
    If guardRef != None
        guardRef.Disable()
    EndIf
EndFunction

Function Fragment_Phase_03_End()
    ObjectReference guardRef = alias_Guard.GetReference()
    ObjectReference exitRef = xmarker_Exit.GetReference()
    If guardRef != None && exitRef != None
        guardRef.MoveTo(exitRef)
    EndIf
EndFunction
