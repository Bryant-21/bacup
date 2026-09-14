Event OnAliasInit()
	Int index = 0
	While index < GetCount()
		ObjectReference activatorRef = GetAt(index)
		If activatorRef != None
			RegisterForRemoteEvent(activatorRef, "OnActivate")
		EndIf
		index += 1
	EndWhile
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() && PlayerCollection != None
		PlayerCollection.AddRef(akActionRef)
	EndIf
EndEvent
