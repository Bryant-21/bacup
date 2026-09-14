Event OnAliasInit()
	Int index = 0
	While index < GetCount()
		ObjectReference triggerRef = GetAt(index)
		If triggerRef != None
			RegisterForRemoteEvent(triggerRef, "OnTriggerEnter")
		EndIf
		index += 1
	EndWhile
EndEvent

Event ObjectReference.OnTriggerEnter(ObjectReference akSender, ObjectReference akActionRef)
	Int actorIndex = 0
	While actorIndex < Actors_Stalkers.GetCount()
		If Actors_Stalkers.GetAt(actorIndex) == akActionRef
			ObjectReference destination = akSender.GetLinkedRef(LinkCustom01)
			If EntranceExplosion != None
				akActionRef.PlaceAtMe(EntranceExplosion)
			EndIf
			If destination != None
				akActionRef.MoveTo(destination)
			EndIf
			If ExitExplosion != None
				akActionRef.PlaceAtMe(ExitExplosion)
			EndIf
			Return
		EndIf
		actorIndex += 1
	EndWhile
EndEvent
