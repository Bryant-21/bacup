Function Fragment_Begin(ObjectReference akSpeakerRef)
    If ConfrontationMODUSTerminal != None && ConfrontationMODUSTerminal.GetRef() != None
        ConfrontationMODUSTerminal.GetRef().BlockActivation(True)
    EndIf
EndFunction

Function Fragment_End(ObjectReference akSpeakerRef)
    If currentPlayer != None && currentPlayer.GetRef() == None
        currentPlayer.ForceRefTo(Game.GetPlayer())
    EndIf
    If ConfrontationMODUSTerminal != None && ConfrontationMODUSTerminal.GetRef() != None
        ConfrontationMODUSTerminal.GetRef().BlockActivation(False)
    EndIf
    If EN02_MQ_Us_0360_FinalScene != None
        EN02_MQ_Us_0360_FinalScene.Start()
    EndIf
EndFunction
