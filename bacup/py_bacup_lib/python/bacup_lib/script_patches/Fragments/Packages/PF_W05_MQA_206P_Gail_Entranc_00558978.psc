Function Fragment_End(Actor akActor)
    Quest owningQuest = GetOwningQuest()
    If owningQuest && !owningQuest.IsStageDone(5011)
        owningQuest.SetStage(5011)
    EndIf
EndFunction
