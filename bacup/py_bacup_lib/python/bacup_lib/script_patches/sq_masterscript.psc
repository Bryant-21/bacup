Event OnQuestInit()
	Debug.Trace("[B21 Daily] SQ_Master OnQuestInit regionManager=" + SQ_RegionManager as String, 0)
	If SQ_RegionManager != None && !SQ_RegionManager.IsRunning()
		SQ_RegionManager.Start()
		Debug.Trace("[B21 Daily] SQ_Master started SQ_RegionManager running=" + SQ_RegionManager.IsRunning() as String, 0)
	ElseIf SQ_RegionManager == None
		Debug.Trace("[B21 Daily] SQ_Master cannot start SQ_RegionManager: binding is None", 0)
	Else
		Debug.Trace("[B21 Daily] SQ_Master found SQ_RegionManager already running", 0)
	EndIf
EndEvent

Int Function GetDailyQuestRegionCount()
	If Regions == None
		Debug.Trace("[B21 Daily] SQ_Master Regions array is None", 0)
		Return 0
	EndIf

	If Regions.Length < 6
		Debug.Trace("[B21 Daily] SQ_Master using short Regions array length=" + Regions.Length as String, 0)
		Return Regions.Length
	EndIf

	Return 6
EndFunction

Int Function GetDailyQuestSelectorCount(Int aiRegionIndex)
	If aiRegionIndex == 0
		Return 2
	ElseIf aiRegionIndex == 1
		Return 2
	ElseIf aiRegionIndex == 2
		Return 2
	ElseIf aiRegionIndex == 3
		Return 3
	ElseIf aiRegionIndex == 4
		Return 4
	ElseIf aiRegionIndex == 5
		Return 5
	EndIf

	Return 0
EndFunction

Function RebuildDailyQuestSequence(Int aiRegionIndex)
	regionData currentRegion = Regions[aiRegionIndex]
	Int questCount = GetDailyQuestSelectorCount(aiRegionIndex)

	If currentRegion.dailyQuestMasterList == None || currentRegion.dailyQuestListSequence == None
		Debug.Trace("[B21 Daily] SQ_Master cannot rebuild sequence region=" + aiRegionIndex as String + " masterList=" + currentRegion.dailyQuestMasterList as String + " sequence=" + currentRegion.dailyQuestListSequence as String, 0)
		Return
	EndIf

	If currentRegion.dailyQuestMasterList.GetSize() < questCount
		Debug.Trace("[B21 Daily] SQ_Master master list too short region=" + aiRegionIndex as String + " size=" + currentRegion.dailyQuestMasterList.GetSize() as String + " expected=" + questCount as String, 0)
		Return
	EndIf

	If currentRegion.dailyQuestListSequence.GetSize() == questCount
		Return
	EndIf

	currentRegion.dailyQuestListSequence.Revert()

	Int questIndex = 0
	While questIndex < questCount
		Form dailyQuest = currentRegion.dailyQuestMasterList.GetAt(questIndex)
		If dailyQuest != None
			currentRegion.dailyQuestListSequence.AddForm(dailyQuest)
		EndIf
		questIndex += 1
	EndWhile
	Debug.Trace("[B21 Daily] SQ_Master rebuilt sequence region=" + aiRegionIndex as String + " size=" + currentRegion.dailyQuestListSequence.GetSize() as String, 0)
EndFunction

Quest Function GetSelectedDailyQuest(Int aiRegionIndex)
	regionData currentRegion = Regions[aiRegionIndex]
	Int questCount = GetDailyQuestSelectorCount(aiRegionIndex)

	If questCount <= 0 || currentRegion.dailyQuestCurrentID == None || currentRegion.dailyQuestListSequence == None
		Debug.Trace("[B21 Daily] SQ_Master cannot select quest region=" + aiRegionIndex as String + " count=" + questCount as String + " currentID=" + currentRegion.dailyQuestCurrentID as String + " sequence=" + currentRegion.dailyQuestListSequence as String, 0)
		Return None
	EndIf

	RebuildDailyQuestSequence(aiRegionIndex)
	If currentRegion.dailyQuestListSequence.GetSize() != questCount
		Debug.Trace("[B21 Daily] SQ_Master sequence size mismatch region=" + aiRegionIndex as String + " size=" + currentRegion.dailyQuestListSequence.GetSize() as String + " expected=" + questCount as String, 0)
		Return None
	EndIf

	Int selectedIndex = currentRegion.dailyQuestCurrentID.GetValueInt()
	If selectedIndex < 0
		selectedIndex = 0
	ElseIf selectedIndex >= questCount
		selectedIndex = selectedIndex % questCount
	EndIf

	currentRegion.dailyQuestCurrentID.SetValue(selectedIndex as Float)
	Quest selectedQuest = currentRegion.dailyQuestListSequence.GetAt(selectedIndex) as Quest
	Debug.Trace("[B21 Daily] SQ_Master selected quest region=" + aiRegionIndex as String + " index=" + selectedIndex as String + " quest=" + selectedQuest as String, 0)
	Return selectedQuest
EndFunction

Function InitializeDailyQuestSchedule(Int aiToday)
	Int regionIndex = 0
	Int regionCount = GetDailyQuestRegionCount()
	Debug.Trace("[B21 Daily] SQ_Master initializing schedule day=" + aiToday as String + " regions=" + regionCount as String, 0)

	While regionIndex < regionCount
		regionData currentRegion = Regions[regionIndex]
		Quest selectedQuest = GetSelectedDailyQuest(regionIndex)

		If currentRegion.dailyQuestListCurrent != None
			currentRegion.dailyQuestListCurrent.Revert()
			If selectedQuest != None
				currentRegion.dailyQuestListCurrent.AddForm(selectedQuest)
			EndIf
		EndIf

		regionIndex += 1
	EndWhile

	ResetBetterTomorrowDaily()
	SQ_TimestampToday.SetValue(aiToday as Float)
	RefreshCostaBusinessEligibility()
EndFunction

Function AdvanceDailyQuestSchedule(Int aiToday, Int aiPreviousDay)
	Int elapsedDays = aiToday - aiPreviousDay
	Int regionIndex = 0
	Int regionCount = GetDailyQuestRegionCount()
	Debug.Trace("[B21 Daily] SQ_Master advancing schedule previousDay=" + aiPreviousDay as String + " today=" + aiToday as String + " elapsed=" + elapsedDays as String, 0)

	While regionIndex < regionCount
		regionData currentRegion = Regions[regionIndex]
		Int questCount = GetDailyQuestSelectorCount(regionIndex)

		RebuildDailyQuestSequence(regionIndex)

		If questCount > 0 && currentRegion.dailyQuestCurrentID != None && currentRegion.dailyQuestListSequence != None && currentRegion.dailyQuestListSequence.GetSize() == questCount
			Int previousIndex = currentRegion.dailyQuestCurrentID.GetValueInt()
			If previousIndex < 0
				previousIndex = 0
			ElseIf previousIndex >= questCount
				previousIndex = previousIndex % questCount
			EndIf

			Quest previousQuest = currentRegion.dailyQuestListSequence.GetAt(previousIndex) as Quest
			If previousQuest != None && (previousQuest.IsCompleted() || previousQuest.IsStopped())
				previousQuest.Reset()
			EndIf

			Int selectedIndex = (previousIndex + elapsedDays) % questCount
			currentRegion.dailyQuestCurrentID.SetValue(selectedIndex as Float)

			If currentRegion.dailyQuestListCurrent != None
				currentRegion.dailyQuestListCurrent.Revert()
				Quest selectedQuest = currentRegion.dailyQuestListSequence.GetAt(selectedIndex) as Quest
				If selectedQuest != None
					currentRegion.dailyQuestListCurrent.AddForm(selectedQuest)
				EndIf
			EndIf
		EndIf

		regionIndex += 1
	EndWhile

	ResetBetterTomorrowDaily()
	SQ_TimestampToday.SetValue(aiToday as Float)
	RefreshCostaBusinessEligibility()
EndFunction

Function ResetBetterTomorrowDaily()
	Quest betterTomorrowQuest = Game.GetFormFromFile(0x006FD072, "SeventySix.esm") as Quest
	If betterTomorrowQuest == None
		Debug.Trace("[B21 Daily] Better Tomorrow reset skipped: quest is None", 0)
	ElseIf !betterTomorrowQuest.IsRunning() && (betterTomorrowQuest.IsCompleted() || betterTomorrowQuest.IsStopped())
		betterTomorrowQuest.Reset()
		Debug.Trace("[B21 Daily] Better Tomorrow reset for new game day", 0)
	EndIf
EndFunction

Function TryStartBetterTomorrow(Location akPlayerLocation, Actor akPlayerRef)
	Quest betterTomorrowQuest = Game.GetFormFromFile(0x006FD072, "SeventySix.esm") as Quest
	Keyword betterTomorrowStartKeyword = Game.GetFormFromFile(0x0072C956, "SeventySix.esm") as Keyword
	Location betterTomorrowRegion = Game.GetFormFromFile(0x00095004, "SeventySix.esm") as Location
	Keyword regionLocationType = Game.GetFormFromFile(0x0009B044, "Fallout4.esm") as Keyword

	If betterTomorrowQuest == None || betterTomorrowStartKeyword == None || betterTomorrowRegion == None
		Debug.Trace("[B21 Daily] Better Tomorrow bindings missing quest=" + betterTomorrowQuest as String + " keyword=" + betterTomorrowStartKeyword as String + " region=" + betterTomorrowRegion as String, 0)
		Return
	EndIf

	Bool playerIsInRegion = akPlayerLocation == betterTomorrowRegion
	If !playerIsInRegion && regionLocationType != None
		playerIsInRegion = akPlayerLocation.IsSameLocation(betterTomorrowRegion, regionLocationType)
	EndIf
	Debug.Trace("[B21 Daily] Better Tomorrow region check location=" + akPlayerLocation as String + " region=" + betterTomorrowRegion as String + " match=" + playerIsInRegion as String, 0)

	If !playerIsInRegion || betterTomorrowQuest.IsRunning() || betterTomorrowQuest.IsCompleted()
		Return
	EndIf

	Bool startedBetterTomorrow = betterTomorrowStartKeyword.SendStoryEventAndWait(betterTomorrowRegion, akPlayerRef)
	Debug.Trace("[B21 Daily] Better Tomorrow Story Manager result=" + startedBetterTomorrow as String + " running=" + betterTomorrowQuest.IsRunning() as String + " stage=" + betterTomorrowQuest.GetStage() as String, 0)
EndFunction

; The namespaced script type must not be stored in a local: the emitted local is
; unresolvable at runtime ("Failed to find variable"), which aborts this call and
; everything after it in the caller. Inline the cast instead.
Function RefreshCostaBusinessEligibility()
	Quest costaDialogueQuest = Game.GetFormFromFile(0x0056B640, "SeventySix.esm") as Quest
	If costaDialogueQuest as Quests:E05_Caravan:Master_QuestScript
		(costaDialogueQuest as Quests:E05_Caravan:Master_QuestScript).RefreshCostaBusinessEligibility()
	EndIf
EndFunction

Function UpdateDailyQuestSchedule()
	If SQ_TimestampToday == None
		Debug.Trace("[B21 Daily] SQ_Master cannot update schedule: SQ_TimestampToday is None", 0)
		Return
	EndIf

	Int today = (Utility.GetCurrentGameTime() as Int) + 1
	Int previousDay = SQ_TimestampToday.GetValueInt()
	Debug.Trace("[B21 Daily] SQ_Master schedule check previousDay=" + previousDay as String + " today=" + today as String, 0)

	If previousDay <= 0
		InitializeDailyQuestSchedule(today)
	ElseIf today > previousDay
		AdvanceDailyQuestSchedule(today, previousDay)
	EndIf
EndFunction

Function TryStartDailyQuest(Location akPlayerLocation, Keyword akStoryKeyword)
	Debug.Trace("[B21 Daily] SQ_Master TryStart location=" + akPlayerLocation as String + " keyword=" + akStoryKeyword as String, 0)
	If akPlayerLocation == None
		Debug.Trace("[B21 Daily] SQ_Master TryStart aborted: player location is None", 0)
		Return
	ElseIf akStoryKeyword == None
		Debug.Trace("[B21 Daily] SQ_Master TryStart aborted: story keyword is None", 0)
		Return
	EndIf

	Actor playerRef = Game.GetPlayer()
	If playerRef == None
		Debug.Trace("[B21 Daily] SQ_Master TryStart aborted: player is None", 0)
		Return
	EndIf

	UpdateDailyQuestSchedule()
	TryStartBetterTomorrow(akPlayerLocation, playerRef)

	Int regionIndex = 0
	Int regionCount = GetDailyQuestRegionCount()
	Debug.Trace("[B21 Daily] SQ_Master evaluating regions count=" + regionCount as String, 0)

	While regionIndex < regionCount
		regionData currentRegion = Regions[regionIndex]
		Bool playerIsInRegion = akPlayerLocation == currentRegion.regionLocation

		If !playerIsInRegion && currentRegion.regionLocation != None && currentRegion.regionTopLevelLocTypeKeyword != None
			playerIsInRegion = akPlayerLocation.IsSameLocation(currentRegion.regionLocation, currentRegion.regionTopLevelLocTypeKeyword)
		EndIf
		Debug.Trace("[B21 Daily] SQ_Master region check index=" + regionIndex as String + " regionLocation=" + currentRegion.regionLocation as String + " topKeyword=" + currentRegion.regionTopLevelLocTypeKeyword as String + " match=" + playerIsInRegion as String, 0)

		If playerIsInRegion
			Debug.Trace("[B21 Daily] SQ_Master matched region index=" + regionIndex as String, 0)
			Quest selectedQuest = GetSelectedDailyQuest(regionIndex)
			If selectedQuest == None
				Debug.Trace("[B21 Daily] SQ_Master matched region has no selected quest", 0)
			ElseIf currentRegion.dailyQuestListCurrent == None
				Debug.Trace("[B21 Daily] SQ_Master matched region has no current daily FormList selected=" + selectedQuest as String, 0)
			ElseIf !currentRegion.dailyQuestListCurrent.HasForm(selectedQuest)
				Debug.Trace("[B21 Daily] SQ_Master selected quest is not eligible today quest=" + selectedQuest as String, 0)
			Else
				Debug.Trace("[B21 Daily] SQ_Master dispatch candidate quest=" + selectedQuest as String + " running=" + selectedQuest.IsRunning() as String + " completed=" + selectedQuest.IsCompleted() as String + " stopped=" + selectedQuest.IsStopped() as String, 0)
				currentRegion.dailyQuestListCurrent.RemoveAddedForm(selectedQuest)

				If !selectedQuest.IsRunning()
					If selectedQuest.IsCompleted() || selectedQuest.IsStopped()
						Debug.Trace("[B21 Daily] SQ_Master resetting selected quest=" + selectedQuest as String, 0)
						selectedQuest.Reset()
					EndIf

					Bool startedDailyQuest = akStoryKeyword.SendStoryEventAndWait(currentRegion.regionLocation, playerRef)
					Debug.Trace("[B21 Daily] SQ_Master Story Manager result=" + startedDailyQuest as String + " selected=" + selectedQuest as String + " running=" + selectedQuest.IsRunning() as String + " stage=" + selectedQuest.GetStage() as String, 0)
					If !startedDailyQuest && GetSelectedDailyQuest(regionIndex) == selectedQuest && !currentRegion.dailyQuestListCurrent.HasForm(selectedQuest)
						currentRegion.dailyQuestListCurrent.AddForm(selectedQuest)
						Debug.Trace("[B21 Daily] SQ_Master restored failed candidate to current list quest=" + selectedQuest as String, 0)
					EndIf
				Else
					Debug.Trace("[B21 Daily] SQ_Master selected quest already running; no Story Manager event sent quest=" + selectedQuest as String, 0)
				EndIf
			EndIf
			Return
		EndIf

		regionIndex += 1
	EndWhile
	Debug.Trace("[B21 Daily] SQ_Master found no matching region for location=" + akPlayerLocation as String, 0)
EndFunction
