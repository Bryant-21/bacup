Event OnAliasInit()
	If Alias_WeedKillers != None
		Int index = 0
		While index < Alias_WeedKillers.GetCount()
			ObjectReference weedKillerRef = Alias_WeedKillers.GetAt(index)
			If weedKillerRef != None
				RegisterForRemoteEvent(weedKillerRef, "OnActivate")
			EndIf
			index += 1
		EndWhile
	EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() && WeedKiller != None
		akActionRef.AddItem(WeedKiller, 1, False)
	EndIf
EndEvent
