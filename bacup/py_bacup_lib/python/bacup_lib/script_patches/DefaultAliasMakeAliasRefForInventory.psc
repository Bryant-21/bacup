Event OnAliasInit()
    OwningQuest = GetOwningQuest()
    If TargetObject != None
        AddInventoryEventFilter(TargetObject)
    EndIf
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akBaseItem != TargetObject || akItemReference == None
        Return
    EndIf

    If OwningQuest == None
        OwningQuest = GetOwningQuest()
    EndIf

    If OwningQuest != None
        If StageToBeginTracking >= 0 && !OwningQuest.IsStageDone(StageToBeginTracking)
            Return
        EndIf
        If ShutdownStage >= 0 && OwningQuest.IsStageDone(ShutdownStage)
            Return
        EndIf
    EndIf

    If TargetAlias != None
        If !TriggerOnce || TargetAlias.GetReference() == None
            TargetAlias.ForceRefTo(akItemReference)
        EndIf
    ElseIf TargetCollection != None && TargetCollection.Find(akItemReference) < 0
        TargetCollection.AddRef(akItemReference)
    EndIf

    If OwningQuest != None && StageToSet >= 0
        OwningQuest.SetStage(StageToSet)
    EndIf
EndEvent

Event OnAliasShutdown()
    RemoveAllInventoryEventFilters()
EndEvent
