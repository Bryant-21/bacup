Event OnAliasInit()
	ResetTechnicalDocumentListener()
EndEvent

Event OnPlayerLoadGame()
	ResetTechnicalDocumentListener()
EndEvent

Function ResetTechnicalDocumentListener()
	RemoveAllInventoryEventFilters()
	UnregisterForAllRemoteEvents()
	Actor playerRef = GetActorReference()
	If playerRef != None && BoSTechnicalDocument != None
		AddInventoryEventFilter(BoSTechnicalDocument)
		RegisterForRemoteEvent(playerRef, "OnItemRemoved")
	EndIf
EndFunction

Event ObjectReference.OnItemRemoved(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
	Actor playerRef = GetActorReference()
	Quest owningQuest = GetOwningQuest()
	If akSender == playerRef && akBaseItem == BoSTechnicalDocument && playerRef != None && playerRef.GetItemCount(BoSTechnicalDocument) == 0 && owningQuest != None && !owningQuest.IsStageDone(350)
		owningQuest.SetStage(350)
	EndIf
EndEvent

Event OnAliasShutdown()
	UnregisterForAllRemoteEvents()
	RemoveAllInventoryEventFilters()
EndEvent
