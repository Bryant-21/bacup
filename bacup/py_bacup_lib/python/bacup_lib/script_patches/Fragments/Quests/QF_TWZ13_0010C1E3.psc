Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	ReferenceAlias curatorAlias = GetAlias(0) as ReferenceAlias
	If curatorAlias == None || akSender != curatorAlias.GetReference()
		Return
	EndIf
	If akActionRef != Game.GetPlayer() || !IsStageDone(100) || IsStageDone(200)
		Return
	EndIf
	SetStage(200)
EndEvent

Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(100, True)

	TWZ13_GraveTriggerRef.Enable(False)
	Alias_DirtTriggerBox.TryToDisableNoWait()

	DefaultMultiStateClientSideActivator graveDisplay = Alias_GraveCollectedObjects.GetReference() as DefaultMultiStateClientSideActivator
	If graveDisplay != None
		graveDisplay.ClientPlayAnimation("Start")
	EndIf

	; Stage 200 was set by the curator's FO76 dialogue, which does not survive conversion.
	ReferenceAlias curatorAlias = GetAlias(0) as ReferenceAlias
	If curatorAlias != None && curatorAlias.GetReference() != None
		RegisterForRemoteEvent(curatorAlias.GetReference(), "OnActivate")
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(100, True)
	SetObjectiveDisplayed(200, True)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(200, True)
	SetObjectiveDisplayed(300, True)
	TWZ13_GraveTriggerRef.Enable(False)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveCompleted(300, True)
	SetObjectiveDisplayed(400, True)

	Actor playerRef = Game.GetPlayer()
	ObjectReference remainsRef = Alias_Remains.GetReference()
	If playerRef != None && remainsRef != None
		Form remainsBase = remainsRef.GetBaseObject()
		If remainsBase != None && playerRef.GetItemCount(remainsBase) > 0
			playerRef.RemoveItem(remainsBase, 1, True)
		EndIf
	EndIf

	TWZ13_GraveTriggerRef.Disable(False)
	Alias_DirtTriggerBox.TryToEnableNoWait()

	DefaultMultiStateClientSideActivator graveDisplay = Alias_GraveCollectedObjects.GetReference() as DefaultMultiStateClientSideActivator
	If graveDisplay != None
		graveDisplay.ClientPlayAnimation("Remains")
	EndIf

	; One shovel per run only: the previous run's shovel is still in the player's inventory.
	ObjectReference shovelMarker = Alias_ShovelMarker.GetReference()
	If shovelMarker != None && Shovel != None && (playerRef == None || playerRef.GetItemCount(Shovel) == 0)
		shovelMarker.PlaceAtMe(Shovel, 1, False, False, True)
	EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
	SetObjectiveCompleted(400, True)
	Alias_DirtTriggerBox.TryToDisableNoWait()

	DefaultMultiStateClientSideActivator graveDisplay = Alias_GraveCollectedObjects.GetReference() as DefaultMultiStateClientSideActivator
	If graveDisplay != None
		graveDisplay.ClientPlayAnimation("DirtMound")
	EndIf

	SetStage(1000)
EndFunction

Function Fragment_Stage_1000_Item_00()
	ReferenceAlias curatorAlias = GetAlias(0) as ReferenceAlias
	If curatorAlias != None && curatorAlias.GetReference() != None
		UnregisterForRemoteEvent(curatorAlias.GetReference(), "OnActivate")
	EndIf
	Stop()
EndFunction
