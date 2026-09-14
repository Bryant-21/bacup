Function ResetDepositedFluid()
	If FFZ10_Light_Global != None
		FFZ10_Light_Global.SetValue(0.0)
	EndIf

	ObjectReference depositContainer = GetReference()
	If depositContainer != None && BiolumFluid01 != None
		Int storedCount = depositContainer.GetItemCount(BiolumFluid01)
		If storedCount > 0
			depositContainer.RemoveItem(BiolumFluid01, storedCount, True)
		EndIf
	EndIf
EndFunction

Event OnAliasInit()
	RemoveAllInventoryEventFilters()
	ResetDepositedFluid()
	If BiolumFluid01 != None
		AddInventoryEventFilter(BiolumFluid01)
	EndIf
EndEvent

Event OnAliasReset()
	RemoveAllInventoryEventFilters()
	ResetDepositedFluid()
	If BiolumFluid01 != None
		AddInventoryEventFilter(BiolumFluid01)
	EndIf
EndEvent

Event OnAliasShutdown()
	RemoveAllInventoryEventFilters()
	If FFZ10_Light_Global != None
		FFZ10_Light_Global.SetValue(0.0)
	EndIf
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
	If BiolumFluid01 == None || akBaseItem != BiolumFluid01 || aiItemCount <= 0
		Return
	EndIf

	ObjectReference depositContainer = GetReference()
	Quest owningQuest = GetOwningQuest()
	If depositContainer == None || owningQuest == None
		Return
	EndIf

	Int currentCount = depositContainer.GetItemCount(BiolumFluid01)
	If currentCount > 30
		currentCount = 30
	EndIf
	If FFZ10_Light_Global != None
		FFZ10_Light_Global.SetValue(currentCount as Float)
	EndIf
	If currentCount >= 30 && !owningQuest.IsStageDone(200)
		owningQuest.SetStage(200)
		RemoveAllInventoryEventFilters()
	EndIf
EndEvent
