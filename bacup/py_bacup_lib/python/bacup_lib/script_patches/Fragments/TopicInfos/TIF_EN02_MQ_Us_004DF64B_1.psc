Function Fragment_End(ObjectReference akSpeakerRef)
    If ConfrontationMODUSTerminal != None && ConfrontationMODUSTerminal.GetRef() != None
        ConfrontationMODUSTerminal.GetRef().BlockActivation(False)
    EndIf
    If EN02_MQ_Us_0170A_ConfrontationII != None
        EN02_MQ_Us_0170A_ConfrontationII.Start()
    EndIf
EndFunction
