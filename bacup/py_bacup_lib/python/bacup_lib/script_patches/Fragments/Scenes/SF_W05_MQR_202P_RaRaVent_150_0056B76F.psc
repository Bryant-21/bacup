Function Fragment_Phase_01_End()
    W05_MQR_202P_QuestScript controller = GetOwningQuest() as W05_MQR_202P_QuestScript
    If controller != None
        controller.EnableRaRaEntryVent(controller.RaRaVent1500Enter)
    EndIf
EndFunction

Function Fragment_Phase_03_End()
    W05_MQR_202P_QuestScript controller = GetOwningQuest() as W05_MQR_202P_QuestScript
    If controller != None
        controller.FinishLastVentEntry()
    EndIf
EndFunction
