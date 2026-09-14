Function Fragment_End()
    Quest owningQuest = GetOwningQuest()
    If owningQuest && owningQuest.IsStageDone(585) && !owningQuest.IsStageDone(590)
        owningQuest.SetStage(590)
    EndIf
EndFunction
