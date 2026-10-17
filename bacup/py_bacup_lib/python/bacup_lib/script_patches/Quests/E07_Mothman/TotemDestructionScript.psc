Event OnDestructionStageChanged(ObjectReference akSenderRef, Int aiOldStage, Int aiCurrentStage)
    If akSenderRef == None || !akSenderRef.IsDestroyed()
        Return
    EndIf
    QuestScript = GetOwningQuest() as Quests:E07_Mothman:QuestScript
    If QuestScript != None
        QuestScript.TotemDestroyed()
    EndIf
EndEvent
