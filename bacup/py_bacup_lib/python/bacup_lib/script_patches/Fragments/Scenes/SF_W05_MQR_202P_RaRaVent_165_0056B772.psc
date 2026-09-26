Function Fragment_Phase_01_Begin()
    W05_MQR_202P_QuestScript controller = GetOwningQuest() as W05_MQR_202P_QuestScript
    If controller != None
        controller.StopBossVentCycle(False)
        controller.EvaluateRaRaPackage()
    EndIf
EndFunction

Function Fragment_Phase_02_End()
    W05_MQR_202P_QuestScript controller = GetOwningQuest() as W05_MQR_202P_QuestScript
    If controller != None
        controller.MoveRaRaToExitVent(controller.RaRaVent1650Exit, Self as Scene)
    EndIf
EndFunction

Function Fragment_Phase_04_Begin()
    W05_MQR_202P_QuestScript controller = GetOwningQuest() as W05_MQR_202P_QuestScript
    If controller != None
        controller.ClearBossVentAliases()
    EndIf
EndFunction
