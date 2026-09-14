Function Fragment_Phase_01_Begin()
    W05_MQR_202P_QuestScript bossVentController = GetOwningQuest() as W05_MQR_202P_QuestScript
    If bossVentController != None
        bossVentController.BeginBossPeek()
    EndIf
EndFunction

Function Fragment_Phase_02_Begin()
    W05_MQR_202P_QuestScript bossVentController = GetOwningQuest() as W05_MQR_202P_QuestScript
    If bossVentController != None
        bossVentController.DropBossVentItem()
    EndIf
EndFunction

Function Fragment_Phase_08_Begin()
    W05_MQR_202P_QuestScript bossVentController = GetOwningQuest() as W05_MQR_202P_QuestScript
    If bossVentController != None
        bossVentController.EndBossPeek()
    EndIf
EndFunction

Function Fragment_Phase_09_End()
    W05_MQR_202P_QuestScript bossVentController = GetOwningQuest() as W05_MQR_202P_QuestScript
    If bossVentController != None
        bossVentController.FinishBossPeekCycle()
    EndIf
EndFunction
