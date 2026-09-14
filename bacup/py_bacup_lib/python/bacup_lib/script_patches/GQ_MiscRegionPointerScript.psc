Event OnQuestInit()
    SetObjectiveDisplayed(ObjectiveID)
    StartTimer(1.0, 100)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 100
        EvaluateCompletionQuests()
    EndIf
EndEvent

Event OnQuestShutdown()
    CancelTimer(100)
EndEvent

Function EvaluateCompletionQuests()
    If !IsRunning()
        Return
    EndIf
    Int index = 0
    While index < CompletionQuests.Length
        Quest advertised = CompletionQuests[index]
        If advertised && IsAdvertisedQuestReached(advertised)
            SetObjectiveCompleted(ObjectiveID)
            ; GQ_MiscRegionPointer_FF05 003EBB93 carries a CompleteQuest stage and
            ; QuestCompletionXP 098952; stopping alone would forfeit both. FO4's own
            ; misc-objective pointer QF_MS02_000229F7 completes first and then stops,
            ; which is also correct for the stage-less pointers in this family.
            CompleteQuest()
            Stop()
            Return
        EndIf
        index += 1
    EndWhile
    StartTimer(10.0, 100)
EndFunction

Bool Function IsAdvertisedQuestReached(Quest akAdvertised)
    GQ_MiscRegionPointerCompletionScript completion = akAdvertised as GQ_MiscRegionPointerCompletionScript
    If completion
        Return completion.IsMiscObjectiveComplete()
    EndIf
    Return akAdvertised.IsCompleted()
EndFunction
