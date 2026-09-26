Function Fragment_Begin(ObjectReference akSpeakerRef)
    W05_002P_Radical_QuestScript controller = GetOwningQuest() as W05_002P_Radical_QuestScript
    If controller != None
        controller.AdvanceYouFirstCounter(4)
    EndIf
EndFunction
