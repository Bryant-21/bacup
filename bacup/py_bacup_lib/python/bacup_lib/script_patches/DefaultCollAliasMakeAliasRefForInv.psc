Event OnAliasInit()
    OwningQuest = GetOwningQuest()
    spinlock = False
    RemoveAllInventoryEventFilters()
    Actor playerRef = Game.GetPlayer()
    Int index = 0
    While ItemsToWatchFor != None && index < ItemsToWatchFor.Length
        ItemStruct item = ItemsToWatchFor[index]
        If item != None
            item.NumFilled = 0
            If item.TargetObject != None
                AddInventoryEventFilter(item.TargetObject)
                ; Collection handlers receive their members' inventory events, so members need the filter too.
                ; The player joins the event collection later, so the player is filtered up front.
                If playerRef != None
                    playerRef.AddInventoryEventFilter(item.TargetObject)
                EndIf
                Int memberIndex = 0
                While memberIndex < GetCount()
                    ObjectReference memberRef = GetAt(memberIndex)
                    If memberRef != None && memberRef != playerRef
                        memberRef.AddInventoryEventFilter(item.TargetObject)
                    EndIf
                    memberIndex += 1
                EndWhile
            EndIf
        EndIf
        index += 1
    EndWhile
EndEvent

Event OnAliasShutdown()
    RemoveAllInventoryEventFilters()
EndEvent

Bool Function ItemIsTracking(ItemStruct akItem)
    If akItem == None || akItem.TargetObject == None || (akItem.TargetAlias == None && akItem.TargetCollection == None)
        Return False
    EndIf
    If akItem.CountToFill >= 0 && akItem.NumFilled >= akItem.CountToFill
        Return False
    EndIf
    If OwningQuest == None
        Return True
    EndIf
    If akItem.StageToBeginTracking >= 0 && !OwningQuest.IsStageDone(akItem.StageToBeginTracking)
        Return False
    EndIf
    Return akItem.ShutdownStage < 0 || !OwningQuest.IsStageDone(akItem.ShutdownStage)
EndFunction

; Only a reference that survives the pickup can be aliased; FO4 passes None for non-persistent items.
Event OnItemAdded(ObjectReference akSenderRef, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akItemReference == None || ItemsToWatchFor == None
        Return
    EndIf
    If OwningQuest == None
        OwningQuest = GetOwningQuest()
    EndIf
    Int index = 0
    While index < ItemsToWatchFor.Length
        ItemStruct item = ItemsToWatchFor[index]
        If item != None && item.TargetObject == akBaseItem && ItemIsTracking(item)
            If item.TargetAlias != None
                If item.TargetAlias.GetReference() != akItemReference
                    item.NumFilled += 1
                    item.TargetAlias.ForceRefTo(akItemReference)
                EndIf
            ElseIf item.TargetCollection.Find(akItemReference) < 0
                item.NumFilled += 1
                item.TargetCollection.AddRef(akItemReference)
            EndIf
            Return
        EndIf
        index += 1
    EndWhile
EndEvent
