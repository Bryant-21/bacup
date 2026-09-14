Event OnAliasInit()
	ResetFreddyClueFilters()
	ReconcileFreddyClues()
EndEvent

Event OnPlayerLoadGame()
	ResetFreddyClueFilters()
	ReconcileFreddyClues()
EndEvent

Event OnAliasShutdown()
	RemoveAllInventoryEventFilters()
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
	ReconcileFreddyClues()
EndEvent

Function ResetFreddyClueFilters()
	RemoveAllInventoryEventFilters()
	If FreddyClues == None
		Return
	EndIf

	Int i = 0
	While i < FreddyClues.Length
		If FreddyClues[i].ClueForm != None
			AddInventoryEventFilter(FreddyClues[i].ClueForm)
		EndIf
		i += 1
	EndWhile
EndFunction

Function ReconcileFreddyClues()
	Quest owningQuest = GetOwningQuest()
	Actor playerRef = Alias_Player.GetActorReference()
	If owningQuest == None || playerRef == None || FreddyClues == None || FreddyClues.Length == 0
		Return
	EndIf
	If !owningQuest.IsStageDone(PrerequisiteStage) || owningQuest.IsStageDone(FoundCluesStage)
		Return
	EndIf

	Count = 0
	Int i = 0
	While i < FreddyClues.Length
		If FreddyClues[i].ClueForm != None && playerRef.GetItemCount(FreddyClues[i].ClueForm) > 0
			FreddyClues[i].Found = True
			Count += 1
		Else
			FreddyClues[i].Found = False
		EndIf
		i += 1
	EndWhile

	If Count >= FreddyClues.Length
		owningQuest.SetStage(FoundCluesStage)
	EndIf
EndFunction
