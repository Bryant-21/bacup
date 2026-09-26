Function Fragment_Begin(ObjectReference akSpeakerRef)
    W05_MQR_203P_QuestScript controller = GetOwningQuest() as W05_MQR_203P_QuestScript
    If controller != None
        controller.PlayCrowdReaction(75)
    EndIf
EndFunction
