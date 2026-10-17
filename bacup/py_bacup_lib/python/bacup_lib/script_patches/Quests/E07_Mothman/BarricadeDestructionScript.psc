; Each vine collection guards one pyre; its global mirrors the destroyed count for the Observers' dialogue.
Event OnDestructionStageChanged(ObjectReference akSenderRef, Int aiOldStage, Int aiCurrentStage)
    If akSenderRef == None || !akSenderRef.IsDestroyed()
        Return
    EndIf
    Int destroyed = 0
    Int index = 0
    While index < GetCount()
        ObjectReference vine = GetAt(index)
        If vine != None && vine.IsDestroyed()
            destroyed += 1
        EndIf
        index += 1
    EndWhile
    If GlobalToSet != None
        GlobalToSet.SetValue(destroyed)
    EndIf
    Int goal = GoalValue
    If goal > GetCount()
        goal = GetCount()
    EndIf
    Quest owner = GetOwningQuest()
    If destroyed >= goal && owner != None && !owner.IsStageDone(AllDestroyedStage)
        owner.SetStage(AllDestroyedStage)
    EndIf
EndEvent
