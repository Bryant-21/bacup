Event OnAliasInit()
	Int index = 0
	While index < GetCount()
		ObjectReference collectionRef = GetAt(index)
		If collectionRef != None
			RegisterForRemoteEvent(collectionRef, "OnActivate")
		EndIf
		index += 1
	EndWhile
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	If activationLock || ItemToAdd == None || ItemQuantity <= 0 || (PlayerOnly && akActionRef != Game.GetPlayer())
		Return
	EndIf
	If DoNotAddDuplicate && akActionRef.GetItemCount(ItemToAdd) > 0
		Return
	EndIf
	activationLock = True
	akActionRef.AddItem(ItemToAdd, ItemQuantity, False)
	activationLock = False
EndEvent
