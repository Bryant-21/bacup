Function Fragment_Begin(ObjectReference akSpeakerRef)
    If ConfontationMODUSTerminal != None && ConfontationMODUSTerminal.GetRef() != None
        ConfontationMODUSTerminal.GetRef().BlockActivation(True)
    EndIf
EndFunction
