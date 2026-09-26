Event OnAliasInit()
    OwningQuest = GetOwningQuest()
    NumFound = 0
EndEvent

Event OnContainerChanged(ObjectReference akSenderRef, ObjectReference akNewContainer, ObjectReference akOldContainer)
    If akSenderRef == None || akNewContainer != Game.GetPlayer()
        Return
    EndIf
    If OwningQuest == None
        OwningQuest = GetOwningQuest()
    EndIf
    If !OwningQuest.IsStageDone(700) || OwningQuest.IsStageDone(Stage_FoundAllEvidence)
        Return
    EndIf
    ; The keyword hides the item's objective 50 compass target and prevents counting it twice.
    If E01C_Tales_Dark_FoundKeyword != None
        If akSenderRef.HasKeyword(E01C_Tales_Dark_FoundKeyword)
            Return
        EndIf
        akSenderRef.AddKeyword(E01C_Tales_Dark_FoundKeyword)
    EndIf
    NumFound += 1
    If NumFound >= GetCount()
        OwningQuest.SetObjectiveCompleted(Obj_FindEvidence, True)
        OwningQuest.SetStage(Stage_FoundAllEvidence)
    EndIf
EndEvent
