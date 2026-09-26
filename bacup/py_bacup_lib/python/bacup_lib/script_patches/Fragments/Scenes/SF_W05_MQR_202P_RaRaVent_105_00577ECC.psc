Function Fragment_Phase_01_End()
    W05_MQR_202P_QuestScript controller = GetOwningQuest() as W05_MQR_202P_QuestScript
    If controller != None
        controller.MoveRaRaToExitVent(controller.RaRaVent1200SitFurniture, Self as Scene)
    EndIf
EndFunction

Function Fragment_Phase_09_End()
    If GetOwningQuest().IsRunning() && Alias_Turrets01 != None
        Alias_Turrets01.EnableAll()
    EndIf
EndFunction

Function Fragment_Phase_10_Begin()
    W05_MQR_202P_QuestScript controller = GetOwningQuest() as W05_MQR_202P_QuestScript
    If controller != None && controller.IsRunning()
        controller.AssignTurretWaveTargets(controller.Turrets01Aliases, controller.Turrets01Targets)
    EndIf
EndFunction

Function Fragment_Phase_13_Begin()
    W05_MQR_202P_QuestScript controller = GetOwningQuest() as W05_MQR_202P_QuestScript
    If controller != None && controller.IsRunning()
        If controller.Turrets02 != None
            controller.Turrets02.EnableAll()
        EndIf
        controller.AssignTurretWaveTargets(controller.Turrets02Aliases, controller.Turrets02Targets)
    EndIf
EndFunction
