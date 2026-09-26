Function Fragment_Phase_01_Begin()
    W05_MQR_202P_QuestScript controller = GetOwningQuest() as W05_MQR_202P_QuestScript
    If controller != None
        controller.EnableRaRaEntryVent(controller.RaRaVent1010Enter)
    EndIf
EndFunction
