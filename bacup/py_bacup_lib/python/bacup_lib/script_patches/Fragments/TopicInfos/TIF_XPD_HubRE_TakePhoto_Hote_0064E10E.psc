Function Fragment_End(ObjectReference akSpeakerRef)
    Quest encounter = GetOwningQuest()
    If encounter == None
        Return
    EndIf
    If !encounter.IsStageDone(230)
        encounter.SetStage(230)
    EndIf
    If !encounter.IsStageDone(300)
        encounter.SetStage(300)
    EndIf
EndFunction
