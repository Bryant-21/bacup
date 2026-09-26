Function Fragment_Phase_01_End()
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && owningQuest.IsRunning() && owningQuest.IsStageDone(8000) && !owningQuest.IsStageDone(8100)
        owningQuest.SetStage(8100)
    EndIf
EndFunction
