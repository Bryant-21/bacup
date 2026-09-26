Event OnAliasInit()
EndEvent

Event OnAliasShutdown()
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
EndEvent

Event OnPlayerLoadGame()
EndEvent

Function OnFishingCatch(Int aiItemCount)
    Quest owningQuest = GetOwningQuest()
    Actor playerRef = GetActorReference()
    If owningQuest == None || !owningQuest.IsRunning() || playerRef == None || FishCaughtAV == None || aiItemCount <= 0
        Return
    EndIf
    If !owningQuest.IsStageDone(500) || owningQuest.IsStageDone(600)
        Return
    EndIf
    playerRef.ModValue(FishCaughtAV, aiItemCount as Float)
    If playerRef.GetValue(FishCaughtAV) >= 3.0
        owningQuest.SetStage(600)
    EndIf
EndFunction
