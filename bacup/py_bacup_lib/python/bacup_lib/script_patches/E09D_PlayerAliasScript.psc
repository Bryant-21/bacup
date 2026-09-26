Event OnAliasInit()
	OwningQuest = GetOwningQuest()
	RegisterInventoryFilters()
EndEvent

Event OnAliasShutdown()
	RemoveAllInventoryEventFilters()
EndEvent

Function RegisterInventoryFilters()
	RemoveAllInventoryEventFilters()
	Int index = 0
	While ItemsToWatch != None && index < ItemsToWatch.Length
		If ItemsToWatch[index] != None
			AddWatchedFilter(ItemsToWatch[index].Item)
		EndIf
		index += 1
	EndWhile
EndFunction

Function AddWatchedFilter(Form akFilter)
	If akFilter == None
		Return
	EndIf
	AddInventoryEventFilter(akFilter)
	; Collection handlers receive their members' inventory events, so the player needs the filter as well.
	; DefaultQuestRemovePlayersScript adds the player after this alias initializes.
	Actor player = Game.GetPlayer()
	If player != None
		player.AddInventoryEventFilter(akFilter)
	EndIf
	Int index = 0
	While index < GetCount()
		ObjectReference memberRef = GetAt(index)
		If memberRef != None && memberRef != player
			memberRef.AddInventoryEventFilter(akFilter)
		EndIf
		index += 1
	EndWhile
EndFunction

Event OnItemAdded(ObjectReference akSenderRef, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
	If akSenderRef != Game.GetPlayer() || akBaseItem == None || ItemsToWatch == None
		Return
	EndIf
	Int row = ItemsToWatch.FindStruct("Item", akBaseItem)
	If row < 0
		Return
	EndIf
	If OwningQuest == None
		OwningQuest = GetOwningQuest()
	EndIf
	ItemStageStruct watched = ItemsToWatch[row]
	If OwningQuest == None || !OwningQuest.IsRunning() || watched.Stage < 0
		Return
	EndIf
	If SetOnce && OwningQuest.IsStageDone(watched.Stage)
		Return
	EndIf
	If watched.TopicToSay != None
		akSenderRef.Say(watched.TopicToSay, None, True)
	EndIf
	OwningQuest.SetStage(watched.Stage)
EndEvent
