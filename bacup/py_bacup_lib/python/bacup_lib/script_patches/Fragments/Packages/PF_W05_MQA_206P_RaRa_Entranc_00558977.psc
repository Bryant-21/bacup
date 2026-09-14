Function Fragment_End(Actor akActor)
    Quest owningQuest = GetOwningQuest()
    If owningQuest && !owningQuest.IsStageDone(5012)
        owningQuest.SetStage(5012)
    EndIf
EndFunction
