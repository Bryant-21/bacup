Event OnDeath(Actor akKiller)
    Quest owner = GetOwningQuest()
    CB15_QuestScript eventQuest = owner as CB15_QuestScript
    If eventQuest != None
        eventQuest.AlphaKilled(GetActorReference())
    EndIf
EndEvent
