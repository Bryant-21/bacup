Function Fragment_End(ObjectReference akSpeakerRef)
    If akSpeakerRef != None && akSpeakerRef != Game.GetPlayer() && ConfrontationMODUSTerminal != None
        ConfrontationMODUSTerminal.ForceRefTo(akSpeakerRef)
    EndIf
    If ConfrontationMODUSTerminal != None && ConfrontationMODUSTerminal.GetRef() != None
        ConfrontationMODUSTerminal.GetRef().BlockActivation(False)
    EndIf
    If EN02_MQ_Us_0170A_ConfrontationII != None && !EN02_MQ_Us_0170A_ConfrontationII.IsPlaying()
        EN02_MQ_Us_0170A_ConfrontationII.Start()
    EndIf
EndFunction
