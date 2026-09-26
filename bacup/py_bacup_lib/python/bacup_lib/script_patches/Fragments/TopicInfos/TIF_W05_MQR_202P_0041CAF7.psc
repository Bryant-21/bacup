Function Fragment_Begin(ObjectReference akSpeakerRef)
    W05_MQR_202P_QuestScript controller = GetOwningQuest() as W05_MQR_202P_QuestScript
    If controller != None
        controller.RequestFoodTransfer()
    EndIf
EndFunction
