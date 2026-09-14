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
	If akActionRef != Game.GetPlayer()
		Return
	EndIf
	Int index = 0
	While index < Items.Length
		If Items[index].Item != None && Items[index].Count > 0
			akActionRef.AddItem(Items[index].Item, Items[index].Count, !ShowItemAddMessage)
		EndIf
		index += 1
	EndWhile
	If BlockAfterActivation
		akSender.BlockActivation(True, True)
	EndIf
	If DisableAfterActivation
		akSender.Disable()
	EndIf
EndEvent
