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

	Return Regions.Length
EndFunction

; The rotation length is the master list, not a constant: the region nodes select
; on GetGlobalValue(<region dailyQuestCurrentID>) == <index into that list>, so any
; index the list holds is a real slot. Measured sizes are 2,3,2,3,4,5,2,1,1,0,0.
Int Function GetDailyQuestSelectorCount(Int aiRegionIndex)
	regionData currentRegion = Regions[aiRegionIndex]
	If currentRegion.dailyQuestMasterList == None
		Return 0
	EndIf

	Return currentRegion.dailyQuestMasterList.GetSize()
EndFunction

; Start gates measured on the converted Story Manager nodes that consume
; SQ_RegionDailyQuestKeyword (117AD8) plus the region location.
;
; These have to be mirrored before the event is sent. An event no node consumes is
; not discarded: the Story Manager keeps walking the script-event root and reaches
; Moon_SQ06_Vera_Branch (6A9F35), whose branch and all seven Costa Business quest
; nodes carry no conditions at all, so the unconsumed daily event starts those
; instead.
Bool Function IsDailyQuestDispatchable(Int aiRegionIndex, Quest akDailyQuest, Actor akPlayerRef)
	If akDailyQuest == None
		Return False
	EndIf

	If aiRegionIndex == 3
		; Ash Heap nodes 3BA041/3BA040/3BA03F gate on
		; GetQuestCompleted(MTR04_Employee "Mistaken Identity") > 0, not on their own quest.
		Quest mtrUnlockQuest = Game.GetFormFromFile(0x001ED40A, "SeventySix.esm") as Quest
		Return mtrUnlockQuest != None && mtrUnlockQuest.IsCompleted()
	EndIf

	If aiRegionIndex == 8
		; 563A72 W05_Daily_Photo_Repeatable also needs GetItemCount(Camera_SnapMatic) != 0.
		Form snapMaticCamera = Game.GetFormFromFile(0x0046F481, "SeventySix.esm")
		If akPlayerRef == None || snapMaticCamera == None || akPlayerRef.GetItemCount(snapMaticCamera) <= 0
			Return False
		EndIf
	EndIf

	If aiRegionIndex == 0 || aiRegionIndex == 5 || aiRegionIndex == 7
		; Cranberry Bog (3BA045/3EBB7B), Toxic Valley (3BA03E..3E8312) and Foundation
		; (597A6C) carry no completion gate, so the selector global is the whole test.
		Return True
	EndIf

	; The Forest (3BA043/3E23DB), Savage Divide (3BA042/3E3C68), the Mire
	; (3BA03B..5105EB), the Crater (5619F7/561CC5) and the Overseer's Home (563A72)
	; are repeat nodes: GetQuestCompleted(<the daily itself>) == 1. The first run has
	; to come from the quest's own first-time node, which is reachable only while the
	; converter carries the quest's FO76 start keyword.
	Return akDailyQuest.IsCompleted() || GetFirstTimeStartRoute(akDailyQuest) != None
EndFunction

; FO76 stored the keyword that offers a quest in QUST.QSSK and sent it from the
; server's daily content manager. FO4 has neither, so the converter carries it on
; B21:QuestStartKeyword; None means this quest has no first-time route at all.
B21:QuestStartKeyword Function GetFirstTimeStartRoute(Quest akDailyQuest)
	B21:QuestStartKeyword startRoute = akDailyQuest as B21:QuestStartKeyword
	If startRoute == None || startRoute.StartKeyword == None
		Return None
	EndIf
	Return startRoute
EndFunction

Function RebuildDailyQuestSequence(Int aiRegionIndex)
	regionData currentRegion = Regions[aiRegionIndex]
	Int questCount = GetDailyQuestSelectorCount(aiRegionIndex)

	If questCount <= 0 || currentRegion.dailyQuestListSequence == None
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
		Return None
	EndIf

	RebuildDailyQuestSequence(aiRegionIndex)
	If currentRegion.dailyQuestListSequence.GetSize() != questCount
		Debug.Trace("[B21 Daily] SQ_Master sequence size mismatch region=" + aiRegionIndex as String + " size=" + currentRegion.dailyQuestListSequence.GetSize() as String + " expected=" + questCount as String, 0)
		Return None
	EndIf

	Int baseIndex = currentRegion.dailyQuestCurrentID.GetValueInt()
	If baseIndex < 0
		baseIndex = 0
	EndIf
	baseIndex = baseIndex % questCount

	Actor playerRef = Game.GetPlayer()
	Int offset = 0
	While offset < questCount
		Int candidateIndex = (baseIndex + offset) % questCount
		Quest candidateQuest = currentRegion.dailyQuestListSequence.GetAt(candidateIndex) as Quest
		If IsDailyQuestDispatchable(aiRegionIndex, candidateQuest, playerRef)
			currentRegion.dailyQuestCurrentID.SetValue(candidateIndex as Float)
			Debug.Trace("[B21 Daily] SQ_Master selected quest region=" + aiRegionIndex as String + " index=" + candidateIndex as String + " quest=" + candidateQuest as String, 0)
			Return candidateQuest
		EndIf
		offset += 1
	EndWhile

	Debug.Trace("[B21 Daily] SQ_Master no dispatchable daily region=" + aiRegionIndex as String + " count=" + questCount as String + " baseIndex=" + baseIndex as String, 0)
	Return None
EndFunction

; Close out yesterday's daily. Nothing in the conversion consumes FO76's
; QuestExpireTimerLength (GLOB 380939), so a run the player walked away from would
; hold its slot forever: the Story Manager will not start a quest that is running.
Function ExpireDailyQuest(Quest akDailyQuest)
	If akDailyQuest == None || !akDailyQuest.IsRunning()
		Return
	EndIf

	akDailyQuest.Stop()
	Debug.Trace("[B21 Daily] SQ_Master expired yesterday's daily quest=" + akDailyQuest as String + " stage=" + akDailyQuest.GetStage() as String, 0)
EndFunction

; Clear yesterday's stages, aliases and counters. Reset() is the only way to replay a
; quest whose stages already ran, but the repeat nodes gate on GetQuestCompleted == 1,
; so the completion flag is restored when Reset() drops it.
Function ResetDailyQuestForNewDay(Quest akDailyQuest)
	If akDailyQuest == None
		Return
	EndIf

	If akDailyQuest.IsRunning()
		; Completed but still running is a dead end: a daily whose completion stage
		; ran without its Stop() (a Stop() reached only through a scene fragment, or
		; one the quest deliberately defers) can never be restarted, because the
		; Story Manager refuses a running quest and the dispatcher skips it. This is
		; the day boundary, so rescue it; an unfinished run is the player's.
		If !akDailyQuest.IsCompleted()
			Return
		EndIf

		akDailyQuest.Stop()
		Debug.Trace("[B21 Daily] SQ_Master stopped a completed-but-running daily quest=" + akDailyQuest as String, 0)
	EndIf

	Bool wasCompleted = akDailyQuest.IsCompleted()
	akDailyQuest.Reset()
	If wasCompleted && !akDailyQuest.IsCompleted()
		akDailyQuest.CompleteQuest()
		Debug.Trace("[B21 Daily] SQ_Master restored completion flag after Reset quest=" + akDailyQuest as String, 0)
	EndIf
EndFunction

Function PublishDailyQuestForRegion(Int aiRegionIndex)
	regionData currentRegion = Regions[aiRegionIndex]
	Quest selectedQuest = GetSelectedDailyQuest(aiRegionIndex)
	ResetDailyQuestForNewDay(selectedQuest)

	If currentRegion.dailyQuestListCurrent != None
		currentRegion.dailyQuestListCurrent.Revert()
		If selectedQuest != None
			currentRegion.dailyQuestListCurrent.AddForm(selectedQuest)
		EndIf
	EndIf
EndFunction

Function InitializeDailyQuestSchedule(Int aiToday)
	Int regionIndex = 0
	Int regionCount = GetDailyQuestRegionCount()
	Debug.Trace("[B21 Daily] SQ_Master initializing schedule day=" + aiToday as String + " regions=" + regionCount as String, 0)

	While regionIndex < regionCount
		PublishDailyQuestForRegion(regionIndex)
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

		If questCount > 0 && currentRegion.dailyQuestCurrentID != None
			Int previousIndex = currentRegion.dailyQuestCurrentID.GetValueInt()
			If previousIndex < 0
				previousIndex = 0
			EndIf
			previousIndex = previousIndex % questCount

			RebuildDailyQuestSequence(regionIndex)
			If currentRegion.dailyQuestListSequence != None && currentRegion.dailyQuestListSequence.GetSize() == questCount
				ExpireDailyQuest(currentRegion.dailyQuestListSequence.GetAt(previousIndex) as Quest)
			EndIf

			currentRegion.dailyQuestCurrentID.SetValue(((previousIndex + elapsedDays) % questCount) as Float)
		EndIf

		PublishDailyQuestForRegion(regionIndex)
		regionIndex += 1
	EndWhile

	AdvanceRefugeDailySchedule(aiPreviousDay)
	ResetBetterTomorrowDaily()
	SQ_TimestampToday.SetValue(aiToday as Float)
	RefreshCostaBusinessEligibility()
EndFunction

; The Whitespring Refuge rotation. FO76 ran it from DCGF
; XPD_Fuel_Whitespring_RandomQuests (64C664), which lists exactly these four quests
; and Location 6240BC; DCGF has no FO4 record type and is dropped wholesale, so
; nothing else sends these keywords.
Quest Function GetRefugeRotationQuest(Int aiRotationIndex)
	If aiRotationIndex == 0
		Return Game.GetFormFromFile(0x0063D33F, "SeventySix.esm") as Quest
	ElseIf aiRotationIndex == 1
		Return Game.GetFormFromFile(0x0063BED4, "SeventySix.esm") as Quest
	ElseIf aiRotationIndex == 2
		Return Game.GetFormFromFile(0x0063D5BD, "SeventySix.esm") as Quest
	EndIf

	Return Game.GetFormFromFile(0x00621FB7, "SeventySix.esm") as Quest
EndFunction

; Each node under XPD_HubQuests_BranchNode (62F619) selects on its own quest's start
; keyword through the same Keyword event-data member the region branch uses.
Keyword Function GetRefugeRotationStartKeyword(Int aiRotationIndex)
	If aiRotationIndex == 0
		Return Game.GetFormFromFile(0x0063D35F, "SeventySix.esm") as Keyword
	ElseIf aiRotationIndex == 1
		Return Game.GetFormFromFile(0x0063BF0E, "SeventySix.esm") as Keyword
	ElseIf aiRotationIndex == 2
		Return Game.GetFormFromFile(0x006422F2, "SeventySix.esm") as Keyword
	EndIf

	Return Game.GetFormFromFile(0x0062FEEB, "SeventySix.esm") as Keyword
EndFunction

Function AdvanceRefugeDailySchedule(Int aiPreviousDay)
	Quest refugeQuest = GetRefugeRotationQuest(aiPreviousDay % 4)
	If refugeQuest == None
		Return
	EndIf

	ExpireDailyQuest(refugeQuest)
	; Stage 0 is the offer latch: these nodes gate on GetQuestRunning / HasKeyword and
	; never on completion, so without the reset the quest would be re-offered the
	; moment the player finished it.
	refugeQuest.Reset()
EndFunction

Function SendRefugeDailyQuest(Quest akRefugeQuest, Keyword akStartKeyword, Location akRefugeLocation)
	If akRefugeQuest == None || akStartKeyword == None
		Debug.Trace("[B21 Daily] SQ_Master refuge daily bindings missing quest=" + akRefugeQuest as String + " keyword=" + akStartKeyword as String, 0)
		Return
	EndIf

	If akRefugeQuest.IsRunning() || akRefugeQuest.GetStage() != 0
		Return
	EndIf

	; No reference is passed: the HasKeyword guards on XPD_Hub_Guide_QuestNode 63D363,
	; XPD_Hub_Mutual_QuestNode 6422F4 and XPD_Fuel_Training_QuestNode 67C279 run on the
	; event subject, and a subject that carries the quest's active keyword fails them.
	Bool startedRefugeQuest = akStartKeyword.SendStoryEventAndWait(akRefugeLocation)
	Debug.Trace("[B21 Daily] SQ_Master refuge daily result=" + startedRefugeQuest as String + " quest=" + akRefugeQuest as String + " running=" + akRefugeQuest.IsRunning() as String + " stage=" + akRefugeQuest.GetStage() as String, 0)
EndFunction

Function TryStartRefugeDaily(Location akPlayerLocation, Int aiToday)
	Location refugeLocation = Game.GetFormFromFile(0x006240BC, "SeventySix.esm") as Location
	Keyword whitespringRegion = Game.GetFormFromFile(0x003B3731, "SeventySix.esm") as Keyword

	If refugeLocation == None
		Debug.Trace("[B21 Daily] SQ_Master refuge location missing", 0)
		Return
	EndIf

	Bool playerIsAtRefuge = akPlayerLocation == refugeLocation
	If !playerIsAtRefuge && whitespringRegion != None
		playerIsAtRefuge = akPlayerLocation.IsSameLocation(refugeLocation, whitespringRegion)
	EndIf

	If !playerIsAtRefuge
		Return
	EndIf

	Int rotationIndex = aiToday % 4
	Debug.Trace("[B21 Daily] SQ_Master refuge daily rotation index=" + rotationIndex as String, 0)
	SendRefugeDailyQuest(GetRefugeRotationQuest(rotationIndex), GetRefugeRotationStartKeyword(rotationIndex), refugeLocation)

	; Training Day is not in the rotation group and its converted QUST carries RunOnce,
	; so it is a one-shot offer rather than a daily.
	SendRefugeDailyQuest(Game.GetFormFromFile(0x0067C1E9, "SeventySix.esm") as Quest, Game.GetFormFromFile(0x0067C265, "SeventySix.esm") as Keyword, refugeLocation)
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

	; Node 7399D6 is synthesized with no PreviousNode, so its position relative to the
	; conditionless Moon_SQ06_Vera_Branch 6A9F35 is undefined. "A quest started, but not
	; this one" is the only in-game signal that the Costa branch ate the event.
	If startedBetterTomorrow && !betterTomorrowQuest.IsRunning()
		Debug.Trace("[B21 Daily] Better Tomorrow event was consumed by another node; quest did not start quest=" + betterTomorrowQuest as String, 0)
	EndIf
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

Function DispatchDailyQuestForRegion(Int aiRegionIndex, Keyword akStoryKeyword, Actor akPlayerRef)
	regionData currentRegion = Regions[aiRegionIndex]
	Quest selectedQuest = GetSelectedDailyQuest(aiRegionIndex)

	If selectedQuest == None
		Return
	ElseIf selectedQuest.IsRunning()
		Debug.Trace("[B21 Daily] SQ_Master selected quest already running region=" + aiRegionIndex as String + " quest=" + selectedQuest as String, 0)
		Return
	ElseIf currentRegion.dailyQuestListCurrent == None || !currentRegion.dailyQuestListCurrent.HasForm(selectedQuest)
		Debug.Trace("[B21 Daily] SQ_Master selected quest is not eligible today region=" + aiRegionIndex as String + " quest=" + selectedQuest as String, 0)
		Return
	EndIf

	; Removing the candidate first is the once-per-day latch: every later location
	; change and pulse sees it as ineligible until the next schedule advance.
	currentRegion.dailyQuestListCurrent.RemoveAddedForm(selectedQuest)

	; A repeat node needs one prior completion, and only the quest's own first-time
	; node can grant it. Offer that route first; the region keyword still runs when it
	; does not take, because a sibling node can consume the event instead.
	If !selectedQuest.IsCompleted()
		B21:QuestStartKeyword startRoute = GetFirstTimeStartRoute(selectedQuest)
		If startRoute != None
			Bool startedFirstRun = startRoute.SendStartEvent(currentRegion.regionLocation, akPlayerRef)
			Debug.Trace("[B21 Daily] SQ_Master first-time route result=" + startedFirstRun as String + " region=" + aiRegionIndex as String + " selected=" + selectedQuest as String, 0)
		EndIf
	EndIf

	Bool anyQuestStarted = selectedQuest.IsRunning()
	If !anyQuestStarted
		anyQuestStarted = akStoryKeyword.SendStoryEventAndWait(currentRegion.regionLocation, akPlayerRef)
	EndIf
	Debug.Trace("[B21 Daily] SQ_Master Story Manager result=" + anyQuestStarted as String + " region=" + aiRegionIndex as String + " selected=" + selectedQuest as String + " running=" + selectedQuest.IsRunning() as String + " stage=" + selectedQuest.GetStage() as String, 0)

	If selectedQuest.IsRunning()
		Return
	EndIf

	If anyQuestStarted
		; Some other node consumed the event. Retrying would keep feeding it, so the
		; candidate stays retired for today and the result is traced instead.
		Debug.Trace("[B21 Daily] SQ_Master daily event was consumed by another node region=" + aiRegionIndex as String + " selected=" + selectedQuest as String, 0)
	ElseIf GetSelectedDailyQuest(aiRegionIndex) == selectedQuest
		currentRegion.dailyQuestListCurrent.AddForm(selectedQuest)
		Debug.Trace("[B21 Daily] SQ_Master restored failed candidate to current list quest=" + selectedQuest as String, 0)
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
	TryStartRefugeDaily(akPlayerLocation, SQ_TimestampToday.GetValueInt())

	Int regionIndex = 0
	Int regionCount = GetDailyQuestRegionCount()
	Debug.Trace("[B21 Daily] SQ_Master evaluating regions count=" + regionCount as String, 0)

	; Every matching region is dispatched, not just the first. The settlement rows
	; (The Crater 0616B3, Foundation 09A128, the Overseer's Home 3F8930) sit inside
	; the surrounding region locations and carry their own branch, selector global and
	; master list, so stopping at the first match would retire them permanently.
	Int matchedRegions = 0
	While regionIndex < regionCount
		regionData currentRegion = Regions[regionIndex]
		Bool playerIsInRegion = akPlayerLocation == currentRegion.regionLocation

		If !playerIsInRegion && currentRegion.regionLocation != None && currentRegion.regionTopLevelLocTypeKeyword != None
			playerIsInRegion = akPlayerLocation.IsSameLocation(currentRegion.regionLocation, currentRegion.regionTopLevelLocTypeKeyword)
		EndIf

		If playerIsInRegion
			matchedRegions += 1
			Debug.Trace("[B21 Daily] SQ_Master matched region index=" + regionIndex as String + " regionLocation=" + currentRegion.regionLocation as String, 0)
			DispatchDailyQuestForRegion(regionIndex, akStoryKeyword, playerRef)
		EndIf

		regionIndex += 1
	EndWhile

	If matchedRegions == 0
		Debug.Trace("[B21 Daily] SQ_Master found no matching region for location=" + akPlayerLocation as String, 0)
	EndIf
EndFunction
