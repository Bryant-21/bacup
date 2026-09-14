Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(100, True)

	TWZ13_GraveTriggerRef.Enable(False)
	Alias_DirtTriggerBox.TryToDisableNoWait()

	DefaultMultiStateClientSideActivator graveDisplay = Alias_GraveCollectedObjects.GetReference() as DefaultMultiStateClientSideActivator
	If graveDisplay != None
		graveDisplay.ClientPlayAnimation("Start")
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
	If playerRef != None && remainsRef != None && playerRef.GetItemCount(remainsRef) > 0
		playerRef.RemoveItem(remainsRef, 1, True)
	EndIf

	TWZ13_GraveTriggerRef.Disable(False)
	Alias_DirtTriggerBox.TryToEnableNoWait()

	DefaultMultiStateClientSideActivator graveDisplay = Alias_GraveCollectedObjects.GetReference() as DefaultMultiStateClientSideActivator
	If graveDisplay != None
		graveDisplay.ClientPlayAnimation("Remains")
	EndIf

	ObjectReference shovelMarker = Alias_ShovelMarker.GetReference()
	If shovelMarker != None && Shovel != None
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
	Stop()
EndFunction
