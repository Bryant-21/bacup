Function Fragment_Phase_04_Begin()
    W05_MQR_202P_QuestScript controller = GetOwningQuest() as W05_MQR_202P_QuestScript
    If controller != None && controller.IsRunning()
        controller.SetStage(controller.RaRaUnPeekStage)
        controller.EvaluateRaRaPackage()
    EndIf
EndFunction

Function Fragment_Phase_04_End()
    W05_MQR_202P_QuestScript controller = GetOwningQuest() as W05_MQR_202P_QuestScript
    If controller != None && controller.IsRunning()
        controller.HideRaRaInVent()
        controller.SetStage(controller.DefeatRobotsNearEndStage)
    EndIf
EndFunction
