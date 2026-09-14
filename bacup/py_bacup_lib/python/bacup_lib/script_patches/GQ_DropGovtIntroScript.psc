Event OnQuestInit()
	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef != None
		RegisterForRemoteEvent(playerRef, "OnItemRemoved")
		AddInventoryEventFilter(GQ_DropGovt01Holotape)
	EndIf
	If !IsStageDone(Stage_FirstObjective)
		SetStage(Stage_FirstObjective)
	EndIf
EndEvent

Event ObjectReference.OnItemRemoved(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
	Actor playerRef = Alias_Player.GetActorReference()
	If akSender == playerRef && akBaseItem == GQ_DropGovt01Holotape && aiItemCount > 0 && IsStageDone(200) && !IsStageDone(Stage_HolotapeUsed)
		CancelTimer(holotapeRemovedTimerID)
		StartTimer(holotapeRemovedTime, holotapeRemovedTimerID)
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID != holotapeRemovedTimerID || !IsStageDone(200) || IsStageDone(Stage_HolotapeUsed)
		Return
	EndIf

	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef != None && playerRef.GetItemCount(GQ_DropGovt01Holotape) == 0
		SetStage(Stage_HolotapeUsed)
	EndIf
EndEvent

Event OnQuestShutdown()
	CancelTimer(holotapeRemovedTimerID)
	Actor playerRef = Alias_Player.GetActorReference()
	If playerRef != None
		UnregisterForRemoteEvent(playerRef, "OnItemRemoved")
	EndIf
	RemoveAllInventoryEventFilters()
EndEvent
