Event OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	If bBusy || akActionRef != Game.GetPlayer() || ItemToGive == None || iItemCountToGive <= 0
		Return
	EndIf
	bBusy = True
	akActionRef.AddItem(ItemToGive, iItemCountToGive, !ShowItemAddMessage)
	If ReferenceCollectionAliasToAddTo != None
		ReferenceCollectionAliasToAddTo.AddRef(akActionRef)
	EndIf
	If BlockAfterActivation
		akSender.BlockActivation(True, True)
	EndIf
	If DisableAfterActivation
		akSender.Disable()
	EndIf
	bBusy = False
EndEvent
