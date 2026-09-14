Function Fragment_End(Actor akActor)
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && owningQuest.IsStageDone(8210) && !owningQuest.IsStageDone(8220)
        owningQuest.SetStage(8220)
    EndIf
EndFunction
