Function Fragment_Phase_02_Begin()
    MTN_MQ_QuestScript owner = GetOwningQuest() as MTN_MQ_QuestScript
    If owner
        owner.bMadiganScenePlayed = True
    EndIf
EndFunction
