Function PrepareLostAndFoundEncounter()
	SelectSceneLocation()
	SelectItemLocation()
	SelectMissingItem()
EndFunction

Function SelectItemLocation()
	If ChosenArea != None && ChosenItemLocation.GetReference() != None
		Return
	EndIf
	If ItemLocs == None || ItemLocs.Length == 0
		Return
	EndIf

	Int startIndex = Utility.RandomInt(0, ItemLocs.Length - 1)
	Int offset = 0
	While offset < ItemLocs.Length
		Int index = (startIndex + offset) % ItemLocs.Length
		ItemLocation candidate = ItemLocs[index]
		If candidate != None && (candidate.MatchingAreaSceneStage <= 0 || IsStageDone(candidate.MatchingAreaSceneStage)) && candidate.AreaLocations != None && candidate.AreaLocations.GetCount() > 0
			ObjectReference marker = candidate.AreaLocations.GetAt(Utility.RandomInt(0, candidate.AreaLocations.GetCount() - 1))
			If marker != None
				ChosenArea = candidate
				ChosenItemLocation.ForceRefTo(marker)
				If candidate.StageToSet > 0 && !IsStageDone(candidate.StageToSet)
					SetStage(candidate.StageToSet)
				EndIf
				Return
			EndIf
		EndIf
		offset += 1
	EndWhile
EndFunction

Function SelectMissingItem()
	If ChosenItem != None && ChosenMissingItem.GetReference() != None
		Return
	EndIf
	If LostItems == None || LostItems.Length == 0 || ChosenItemLocation.GetReference() == None
		Return
	EndIf

	ChosenItem = LostItems[Utility.RandomInt(0, LostItems.Length - 1)]
	If ChosenItem == None || ChosenItem.ItemToFind == None
		Return
	EndIf
	ObjectReference missingItemRef = ChosenItemLocation.GetReference().PlaceAtMe(ChosenItem.ItemToFind, 1, False, False, True)
	If missingItemRef != None
		ChosenMissingItem.ForceRefTo(missingItemRef)
	EndIf
	If ChosenItem.StageToSet > 0 && !IsStageDone(ChosenItem.StageToSet)
		SetStage(ChosenItem.StageToSet)
	EndIf
EndFunction

Function ClearLostAndFoundSelection()
	ObjectReference missingItemRef = ChosenMissingItem.GetReference()
	If missingItemRef != None
		missingItemRef.Disable()
		missingItemRef.Delete()
	EndIf
	ChosenMissingItem.Clear()
	ChosenItemLocation.Clear()
	ChosenItem = None
	ChosenArea = None
	ClearLocalSelection()
EndFunction

Event OnQuestShutdown()
	ClearLostAndFoundSelection()
	Parent.OnQuestShutdown()
EndEvent
