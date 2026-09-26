Function Fragment_Begin(ObjectReference akSpeakerRef)
    W05_MQR_204P_QuestScript controller = GetOwningQuest() as W05_MQR_204P_QuestScript
    If controller != None
        controller.PayInformationBribe(200)
    EndIf
EndFunction
