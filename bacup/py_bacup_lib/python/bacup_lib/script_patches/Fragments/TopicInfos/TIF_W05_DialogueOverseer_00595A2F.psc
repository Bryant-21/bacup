Function Fragment_End(ObjectReference akSpeakerRef)
    If pW05_MQ_101P != None && !pW05_MQ_101P.IsStageDone(155)
        pW05_MQ_101P.SetStage(155)
    EndIf
EndFunction
