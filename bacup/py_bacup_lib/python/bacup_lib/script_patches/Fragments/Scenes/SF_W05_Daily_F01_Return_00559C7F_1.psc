Function Fragment_End()
    ; Stage 9999 notes list "End of scene W05_Daily_F01_Return" as one of its two setters.
    Quest owningQuest = GetOwningQuest()
    If owningQuest && owningQuest.IsRunning() && !owningQuest.IsStageDone(9999)
        owningQuest.SetStage(9999)
    EndIf
EndFunction
