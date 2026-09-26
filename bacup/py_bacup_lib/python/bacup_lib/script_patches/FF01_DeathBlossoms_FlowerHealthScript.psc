Bool Function FlowerIsDestroyed()
    ObjectReference flowerRef = GetReference()
    Return flowerRef != None && flowerRef.IsDestroyed()
EndFunction

Function ReportFlowerDestroyed()
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsRunning()
        Return
    EndIf
    If ObjectiveIndex > 0
        If owningQuest.IsObjectiveFailed(ObjectiveIndex)
            Return
        EndIf
        owningQuest.SetObjectiveFailed(ObjectiveIndex, True)
    EndIf
    FF01_DeathBlossoms_QuestScript eventScript = owningQuest as FF01_DeathBlossoms_QuestScript
    If eventScript != None
        eventScript.FlowerDestroyed(GetReference())
    EndIf
EndFunction

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
    If FlowerIsDestroyed()
        ReportFlowerDestroyed()
    Else
        Parent.OnDestructionStageChanged(aiOldStage, aiCurrentStage)
    EndIf
EndEvent

Event OnLoad()
    Parent.OnLoad()
    If FlowerIsDestroyed()
        ReportFlowerDestroyed()
    EndIf
EndEvent
