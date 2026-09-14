Event OnContainerChanged(ObjectReference akNewContainer, ObjectReference akOldContainer)
	Actor playerRef = Game.GetPlayer()
	If akNewContainer != playerRef || playerRef == None
		Return
	EndIf

	If DirtyLaundryQuest != None && !DirtyLaundryQuest.IsRunning() && Burn_SQ04_StartKeyword != None
		Bool started = Burn_SQ04_StartKeyword.SendStoryEventAndWait(playerRef.GetCurrentLocation(), playerRef, Self)
		If started && Burn_SQ04_DirtyLaundry_Start_Message != None
			Burn_SQ04_DirtyLaundry_Start_Message.Show()
		EndIf
	EndIf
	If DirtyLaundryQuest == None || !DirtyLaundryQuest.IsRunning()
		Return
	EndIf

	playerRef.ModValue(Burn_SQ04_NumberOfCollectables, NumberOfIntelToAdd as Float)
	playerRef.ModValue(Burn_SQ04_CollectablesNotHandedIn, NumberOfIntelToAdd as Float)
	Form baseItem = GetBaseObject()
	If baseItem == None || Burn_SQ04_NonIncrementing == None || !baseItem.HasKeyword(Burn_SQ04_NonIncrementing)
		playerRef.ModValue(Burn_SQ04_TotalCasesCollected, 1.0)
	EndIf

	Burn_SQ04_Collectables_Script tracker = DirtyLaundryQuest as Burn_SQ04_Collectables_Script
	If tracker != None
		tracker.ReconcileCollectableProgress()
	EndIf
EndEvent
