Function ResetRequiredItemCounts()
	If RequiredItems == None
		Return
	EndIf

	Int index = 0
	While index < RequiredItems.Length
		If RequiredItems[index] != None
			RequiredItems[index].CurrentCount = 0
		EndIf
		index += 1
	EndWhile
EndFunction

Event OnAliasInit()
	OwningQuest = GetOwningQuest()
	ResetRequiredItemCounts()
EndEvent

Event OnAliasReset()
	OwningQuest = GetOwningQuest()
	ResetRequiredItemCounts()
EndEvent

Event OnAliasShutdown()
	OwningQuest = None
EndEvent

Event OnActivate(ObjectReference akActionRef)
	If akActionRef == None || akActionRef != Game.GetPlayer() || RequiredItems == None
		Return
	EndIf
	If OwningQuest == None
		OwningQuest = GetOwningQuest()
	EndIf
	If OwningQuest == None
		Return
	EndIf

	ObjectReference depositContainer = GetReference()
	Int index = 0
	While index < RequiredItems.Length
		ItemDatum requiredItem = RequiredItems[index]
		If requiredItem != None && requiredItem.ItemToRemove != None
			Int requiredCount = requiredItem.NumRequired
			If requiredCount < 1
				requiredCount = 1
			EndIf

			Int amountNeeded = requiredCount - requiredItem.CurrentCount
			If amountNeeded > 0
				Int countBefore = akActionRef.GetItemCount(requiredItem.ItemToRemove)
				Int amountToRemove = countBefore
				If amountToRemove > amountNeeded
					amountToRemove = amountNeeded
				EndIf
				If amountToRemove > 0
					akActionRef.RemoveItem(requiredItem.ItemToRemove, amountToRemove, True, depositContainer)
					Int amountRemoved = countBefore - akActionRef.GetItemCount(requiredItem.ItemToRemove)
					If amountRemoved > 0
						requiredItem.CurrentCount += amountRemoved
						If requiredItem.CurrentCount > requiredCount
							requiredItem.CurrentCount = requiredCount
						EndIf
					EndIf
				EndIf
			EndIf

			If requiredItem.CurrentCount >= requiredCount && requiredItem.StageToSet >= 0 && !OwningQuest.IsStageDone(requiredItem.StageToSet)
				OwningQuest.SetStage(requiredItem.StageToSet)
			EndIf
		EndIf
		index += 1
	EndWhile
EndEvent
