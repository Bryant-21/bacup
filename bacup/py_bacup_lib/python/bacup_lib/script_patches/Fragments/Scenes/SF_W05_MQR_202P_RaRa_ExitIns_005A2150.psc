Function Fragment_Phase_02_End()
    W05_MQR_202P_QuestScript controller = GetOwningQuest() as W05_MQR_202P_QuestScript
    If controller != None
        controller.HideRaRaInVent()
    EndIf
EndFunction
