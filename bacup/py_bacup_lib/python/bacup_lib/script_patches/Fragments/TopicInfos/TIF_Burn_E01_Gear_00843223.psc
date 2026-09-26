Function Fragment_End(ObjectReference akSpeakerRef)
    ; Last line of Burn_PE_TamerScrapDone: QUST 7F1E8A stage 420 is set when this scene's final dialogue ends.
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsRunning()
        Return
    EndIf
    If owningQuest.IsStageDone(400) && !owningQuest.IsStageDone(420)
        owningQuest.SetStage(420)
    EndIf
EndFunction
