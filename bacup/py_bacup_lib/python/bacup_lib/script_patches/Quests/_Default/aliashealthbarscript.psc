; FO76 also drew a health bar on ObjectiveIndex; FO4 objectives cannot show one, so only the
; stage hooks are restored.
Event OnDying(Actor akKiller)
    TrySetStage(OnDyingStage)
EndEvent

Event OnEnterBleedout()
    TrySetStage(OnenterBleedoutStage)
EndEvent

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
    ObjectReference target = GetReference()
    If target != None && target.IsDestroyed()
        TrySetStage(OnDestroyedStage)
    EndIf
EndEvent

Event OnWorkshopObjectRepaired(ObjectReference akReference)
    TrySetStage(OnRepairedStage)
EndEvent

Function TrySetStage(Int aiStage)
    If aiStage < 0
        Return
    EndIf
    If myQuest == None
        myQuest = GetOwningQuest()
    EndIf
    If myQuest == None || !myQuest.IsRunning() || myQuest.IsStageDone(aiStage)
        Return
    EndIf
    If PrereqStage >= 0 && !myQuest.IsStageDone(PrereqStage)
        Return
    EndIf
    If TurnOffStage >= 0 && myQuest.IsStageDone(TurnOffStage)
        Return
    EndIf
    myQuest.SetStage(aiStage)
EndFunction
