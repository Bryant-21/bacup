Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    OwningQuest = GetOwningQuest()
    QuestScript = OwningQuest as Quests:E07_Mothman:QuestScript
    If QuestScript != None
        QuestScript.BossKilled()
    EndIf
EndEvent
