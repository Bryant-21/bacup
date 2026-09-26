Function Fragment_Phase_01_End()
    W05_MQR_202P_QuestScript controller = GetOwningQuest() as W05_MQR_202P_QuestScript
    If controller != None
        controller.MoveRaRaToExitVent(controller.RaRaVent1000Exit, Self as Scene)
    EndIf
EndFunction
