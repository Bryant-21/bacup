Function Fragment_Phase_02_End()
    W05_MQR_203P_QuestScript controller = GetOwningQuest() as W05_MQR_203P_QuestScript
    If controller != None && controller.IsRunning() && controller.JohnnyArena != None
        controller.JohnnyArena.TryToDisable()
    EndIf
EndFunction
