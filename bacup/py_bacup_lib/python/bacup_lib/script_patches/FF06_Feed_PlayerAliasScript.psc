Event OnAliasInit()
    RemoveAllInventoryEventFilters()
    Int index = 0
    While IngredientAliases != None && index < IngredientAliases.Length
        If IngredientAliases[index] != None && IngredientAliases[index].QuestIngredient != None
            AddIngredientFilter(IngredientAliases[index].QuestIngredient)
        EndIf
        index += 1
    EndWhile
EndEvent

Event OnAliasShutdown()
    RemoveAllInventoryEventFilters()
EndEvent

Function AddIngredientFilter(Form akIngredient)
    ; Collection handlers receive their members' inventory events, so each member needs the filter too.
    AddInventoryEventFilter(akIngredient)
    Int memberIndex = 0
    While memberIndex < GetCount()
        ObjectReference memberRef = GetAt(memberIndex)
        If memberRef != None
            memberRef.AddInventoryEventFilter(akIngredient)
        EndIf
        memberIndex += 1
    EndWhile
EndFunction

Event OnItemAdded(ObjectReference akSenderRef, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    If akItemReference == None || IngredientAliases == None
        Return
    EndIf
    ; Tracking the picked-up ingredient lets the quest's cleanup remove it from the player when the event ends.
    Int index = 0
    While index < IngredientAliases.Length
        IngredientAlias entry = IngredientAliases[index]
        If entry != None && entry.QuestIngredient == akBaseItem && entry.IngredientAlias != None
            If entry.IngredientAlias.Find(akItemReference) < 0
                entry.IngredientAlias.AddRef(akItemReference)
            EndIf
            Return
        EndIf
        index += 1
    EndWhile
EndEvent
