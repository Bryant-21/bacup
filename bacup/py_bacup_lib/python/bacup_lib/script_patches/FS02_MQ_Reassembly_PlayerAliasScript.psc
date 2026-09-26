Function EvaluateAbbiesBunkerEntry()
	Quest owningQuest = GetOwningQuest()
	Actor playerRef = GetActorReference()
	If owningQuest != None && playerRef != None && owningQuest.GetCurrentStageID() == 25 && playerRef.GetCurrentLocation() == AbbiesBunkerLocation
		owningQuest.SetStage(50)
		If FS02_MQ_Reassembly_AbbieIntroScene != None && !FS02_MQ_Reassembly_AbbieIntroScene.IsPlaying()
			FS02_MQ_Reassembly_AbbieIntroScene.Start()
		EndIf
	EndIf
	; Objective 500 "Return to Abbie's bunker" had no stage setter; 600 starts
	; FS03, whose startup completes this quest.
	If owningQuest != None && playerRef != None && owningQuest.GetStageDone(500) && !owningQuest.GetStageDone(600) && playerRef.GetCurrentLocation() == AbbiesBunkerLocation
		owningQuest.SetStage(600)
	EndIf
EndFunction

Event OnAliasInit()
	ResetTrackedItemFilters()
	EvaluateAbbiesBunkerEntry()
EndEvent

Event OnPlayerLoadGame()
	ResetTrackedItemFilters()
EndEvent

Function ResetTrackedItemFilters()
	RemoveAllInventoryEventFilters()
	If FS01_MQ_Warn_UplinkMiscItem != None
		AddInventoryEventFilter(FS01_MQ_Warn_UplinkMiscItem)
	EndIf
	If FS01_MQ_Warn_UpgradedTransmitter != None
		AddInventoryEventFilter(FS01_MQ_Warn_UpgradedTransmitter)
	EndIf
EndFunction

Event OnLocationChange(Location akOldLoc, Location akNewLoc)
	EvaluateAbbiesBunkerEntry()
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
	If akItemReference == None
		Return
	EndIf
	If akBaseItem == FS01_MQ_Warn_UplinkMiscItem
		If Uplink.GetReference() != akItemReference
			Uplink.ForceRefTo(akItemReference)
		EndIf
	ElseIf akBaseItem == FS01_MQ_Warn_UpgradedTransmitter && UpgradedTransmitters.Find(akItemReference) < 0
		UpgradedTransmitters.AddRef(akItemReference)
	EndIf
EndEvent

Event OnAliasShutdown()
	RemoveAllInventoryEventFilters()
EndEvent
