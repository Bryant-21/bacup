Event OnAliasInit()
    ShadowsTotal = 0
    ShadowsDestroyed = 0
EndEvent

Function ResetShadowCount()
    ShadowsDestroyed = 0
    ShadowsTotal = GetCount()
EndFunction

Event OnDestructionStageChanged(ObjectReference akSenderRef, Int aiOldStage, Int aiCurrentStage)
    If akSenderRef == None || aiCurrentStage < DestroyedStage || Find(akSenderRef) < 0
        Return
    EndIf
    RemoveRef(akSenderRef)
    ShadowsDestroyed += 1
    akSenderRef.Delete()
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsStageDone(300) || owningQuest.IsStageDone(Stage_ShadowsDestroyed)
        Return
    EndIf
    If GetCount() == 0 || ShadowsDestroyed >= ShadowsTotal
        owningQuest.SetObjectiveCompleted(Obj_Shadows, True)
        owningQuest.SetStage(Stage_ShadowsDestroyed)
    EndIf
EndEvent
