Function Fragment_End(ObjectReference akSpeakerRef)
    W05_003P_Muscle_QuestScript controller = GetOwningQuest() as W05_003P_Muscle_QuestScript
    If controller != None
        controller.ApplySkinnerDiscount(1223)
    EndIf
EndFunction
