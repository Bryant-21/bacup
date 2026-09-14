Quest Function ActiveCacheQuestAtPlayer(Actor akPlayer)
	If akPlayer == None
		Return None
	EndIf
	Location playerLocation = akPlayer.GetCurrentLocation()
	Quest[] cacheQuests = new Quest[3]
	cacheQuests[0] = Burn_SQ04_Cache1_Misc
	cacheQuests[1] = Burn_SQ04_Cache2_Misc
	cacheQuests[2] = Burn_SQ04_Cache3_Misc

	Int index = 0
	While index < cacheQuests.Length
		Quest cacheQuest = cacheQuests[index]
		If cacheQuest != None && cacheQuest.IsRunning()
			LocationAlias cacheLocationAlias = cacheQuest.GetAlias(0) as LocationAlias
			If cacheLocationAlias != None && cacheLocationAlias.GetLocation() == playerLocation
				Return cacheQuest
			EndIf
		EndIf
		index += 1
	EndWhile
	Return None
EndFunction

Event OnActivate(ObjectReference akActionRef)
	Actor playerRef = Game.GetPlayer()
	If akActionRef != playerRef || playerRef == None || !IsCache
		Return
	EndIf

	If DirtyLaundryQuest != None && !DirtyLaundryQuest.IsRunning() && Burn_SQ04_StartKeyword != None
		Bool started = Burn_SQ04_StartKeyword.SendStoryEventAndWait(playerRef.GetCurrentLocation(), playerRef, Self)
		If started && Burn_SQ04_DirtyLaundry_Start_Message_Cache != None
			Burn_SQ04_DirtyLaundry_Start_Message_Cache.Show()
		EndIf
	EndIf

	Quest activeCache = ActiveCacheQuestAtPlayer(playerRef)
	If activeCache == None
		If Burn_SQ04_CacheOpenFail_Message != None
			Burn_SQ04_CacheOpenFail_Message.Show()
		EndIf
		If OpenFailSound != None
			OpenFailSound.Play(Self)
		EndIf
		Return
	EndIf
	If activeCache.IsStageDone(200)
		If Burn_SQ04_CacheAlreadyOpened_Message != None
			Burn_SQ04_CacheAlreadyOpened_Message.Show()
		EndIf
		Return
	EndIf

	GotoState("active")
	If CacheRewards != None
		playerRef.AddItem(CacheRewards, 1, False)
	EndIf
	activeCache.SetStage(200)
	BlockActivation(True)

	Burn_SQ04_Collectables_Script tracker = DirtyLaundryQuest as Burn_SQ04_Collectables_Script
	If tracker != None
		tracker.ReconcileCollectableProgress()
	EndIf
EndEvent
