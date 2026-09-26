Event OnAliasInit()
    ResolveEventScript()
    RemoveAllInventoryEventFilters()
    If QS != None && QS.ShepherdsCrook != None
        AddInventoryEventFilter(QS.ShepherdsCrook)
        ; Collection handlers receive their members' inventory events, so each member needs the filter too.
        Int memberIndex = 0
        While memberIndex < GetCount()
            ObjectReference memberRef = GetAt(memberIndex)
            If memberRef != None
                memberRef.AddInventoryEventFilter(QS.ShepherdsCrook)
            EndIf
            memberIndex += 1
        EndWhile
    EndIf
EndEvent

Function ResolveEventScript()
    OwningQuest = GetOwningQuest()
    QS = OwningQuest as Quests:E01B_Herd:QuestScript
EndFunction

Event OnItemAdded(ObjectReference akSenderRef, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If QS == None
        ResolveEventScript()
    EndIf
    If QS == None || akSenderRef == None || akSenderRef != Game.GetPlayer() || akBaseItem != QS.ShepherdsCrook
        Return
    EndIf
    If OwningQuest.IsRunning() && OwningQuest.IsStageDone(100) && !OwningQuest.IsStageDone(QS.Stage_ActivityStart)
        OwningQuest.SetStage(QS.Stage_ActivityStart)
    EndIf
EndEvent

Event OnAliasShutdown()
    RemoveAllInventoryEventFilters()
EndEvent
