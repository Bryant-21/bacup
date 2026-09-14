Function Fragment_End(Actor akActor)
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && owningQuest.IsStageDone(200) && !owningQuest.IsStageDone(300)
        owningQuest.SetStage(300)
    EndIf
EndFunction
