Function Fragment_End()
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && owningQuest.IsRunning() && owningQuest.IsStageDone(1100) && !owningQuest.IsStageDone(1101)
        owningQuest.SetStage(1101)
    EndIf
EndFunction
