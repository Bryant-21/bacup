Event OnAliasInit()
    OwningQuest = GetOwningQuest()
    If ItemsToWatch == None
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    Int index = 0
    While index < ItemsToWatch.Length
        If ItemsToWatch[index] != None && ItemsToWatch[index].Item != None
            Form watched = ItemsToWatch[index].Item
            AddInventoryEventFilter(watched)
            ; Collection handlers receive their members' inventory events, so members need the filter too.
            ; The player can join the collection after init, so the player is filtered up front.
            If playerRef != None
                playerRef.AddInventoryEventFilter(watched)
            EndIf
            Int memberIndex = 0
            While memberIndex < GetCount()
                ObjectReference memberRef = GetAt(memberIndex)
                If memberRef != None && memberRef != playerRef
                    memberRef.AddInventoryEventFilter(watched)
                EndIf
                memberIndex += 1
            EndWhile
        EndIf
        index += 1
    EndWhile
EndEvent

Event OnItemAdded(ObjectReference akSenderRef, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If ItemsToWatch == None || akBaseItem == None
        Return
    EndIf
    If OwningQuest == None
        OwningQuest = GetOwningQuest()
    EndIf
    If OwningQuest == None || !OwningQuest.IsRunning()
        Return
    EndIf
    Int index = 0
    While index < ItemsToWatch.Length
        ItemStageStruct entry = ItemsToWatch[index]
        If entry != None && entry.Item == akBaseItem && entry.Stage >= 0
            If !SetOnce || !OwningQuest.IsStageDone(entry.Stage)
                OwningQuest.SetStage(entry.Stage)
            EndIf
        EndIf
        index += 1
    EndWhile
EndEvent

Event OnAliasShutdown()
    RemoveAllInventoryEventFilters()
EndEvent
