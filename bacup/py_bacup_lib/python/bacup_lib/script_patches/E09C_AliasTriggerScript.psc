Event OnActivate(ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer() || ItemToRemove == None || iItemsToRemove <= 0
		Return
	EndIf
	If akActionRef.GetItemCount(ItemToRemove) < iItemsToRemove
		If NotEnoughItemMSG != None
			NotEnoughItemMSG.Show()
		EndIf
		Return
	EndIf
	akActionRef.RemoveItem(ItemToRemove, iItemsToRemove, True)
	If LinkedAlias != None && LinkedAlias.GetReference() != None
		LinkedAlias.GetReference().Enable()
	EndIf
	If sFunctionToCall != ""
		Var[] noArguments = new Var[0]
		GetOwningQuest().CallFunctionNoWait(sFunctionToCall, noArguments)
	EndIf
	If bDisableOnActivate && GetReference() != None
		GetReference().Disable()
	EndIf
EndEvent
