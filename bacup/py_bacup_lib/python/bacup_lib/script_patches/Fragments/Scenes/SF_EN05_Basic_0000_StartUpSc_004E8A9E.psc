Function Fragment_Begin()
    EN05_QuestScript basic = GetOwningQuest() as EN05_QuestScript
    If basic != None
        basic.bKickOffStartUpScene = False
        basic.EN05Basic_ReconcileUniform()
    EndIf
EndFunction

Function Fragment_End()
    EN05_QuestScript basic = GetOwningQuest() as EN05_QuestScript
    If basic != None
        basic.bKickOffStartUpScene = False
    EndIf
EndFunction
