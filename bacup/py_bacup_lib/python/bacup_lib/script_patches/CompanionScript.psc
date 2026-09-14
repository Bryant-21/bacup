Event OnInit()
	InitializeSinglePlayerState()
EndEvent

Event OnLoad()
	InitializeSinglePlayerState()
EndEvent

Function InitializeSinglePlayerState()
	If initializing
		Return
	EndIf
	initializing = True
	COMP_RQ_MasterIns = COMP_RQ_Master as COMP_RQ_MasterScript
	SynchronizeActorValues()
	initializing = False
EndFunction

Function SynchronizeActorValues()
	If SyncActorValueLock || SyncedActorValueData == None
		Return
	EndIf

	Actor player = Game.GetPlayer()
	If !player
		Return
	EndIf
	SyncActorValueLock = True
	Int index = 0
	While index < SyncedActorValueData.Length
		SyncedActorValueDatum currentDatum = SyncedActorValueData[index]
		If currentDatum.AllyAV && currentDatum.PlayerAV
			Float synchronizedValue = GetValue(currentDatum.AllyAV)
			If player.GetValue(currentDatum.PlayerAV) > synchronizedValue
				synchronizedValue = player.GetValue(currentDatum.PlayerAV)
			EndIf
			SetValue(currentDatum.AllyAV, synchronizedValue)
			player.SetValue(currentDatum.PlayerAV, synchronizedValue)
		EndIf
		index += 1
	EndWhile
	SyncActorValueLock = False
EndFunction

Function StartDailyQuest()
	If StartDailyQuestLock || RadiantQuestData == None
		Return
	EndIf
	If GlobalToggle && GlobalToggle.GetValue() <= 0.0
		Return
	EndIf

	Actor player = Game.GetPlayer()
	If !player
		Return
	EndIf
	StartDailyQuestLock = True
	SynchronizeActorValues()
	Int dataIndex = FindRadiantQuestDataIndex(player.GetValue(PlayerQuestCountAV))
	If dataIndex >= 0
		StartRadiantQuestByIndex(dataIndex)
	EndIf
	StartDailyQuestLock = False
EndFunction

Int Function FindRadiantQuestDataIndex(Float questCount)
	Int selectedIndex = -1
	Int eligibleCount = 0
	Int index = 0
	While index < RadiantQuestData.Length
		RadiantQuestDatum currentDatum = RadiantQuestData[index]
		Bool countMatches = questCount == currentDatum.QuestCount
		If currentDatum.QuestCountEqualOrGreaterThan
			countMatches = questCount >= currentDatum.QuestCount
		EndIf
		If countMatches && (currentDatum.QuestCountMax < 0 || questCount <= currentDatum.QuestCountMax)
			If currentDatum.IgnoreCoolDown || GetValue(COMP_DailyQuest_NextAllowedDay) <= Utility.GetCurrentGameTime()
				eligibleCount += 1
				If Utility.RandomInt(1, eligibleCount) == 1
					selectedIndex = index
				EndIf
			EndIf
		EndIf
		index += 1
	EndWhile
	Return selectedIndex
EndFunction

Bool Function StartRadiantQuestByIndex(Int dataIndex)
	If dataIndex < 0 || dataIndex >= RadiantQuestData.Length
		Return False
	EndIf

	Actor player = Game.GetPlayer()
	RadiantQuestDatum currentDatum = RadiantQuestData[dataIndex]
	If !player || !currentDatum.akQuestTarget
		Return False
	EndIf

	RadiantQuestExtraData extraData = new RadiantQuestExtraData
	extraData.CustomFirstObjective = currentDatum.CustomFirstObjective
	extraData.CustomSecondObjective = currentDatum.CustomSecondObjective
	extraData.QuestNameOverrideLocation = currentDatum.QuestNameOverrideLocation
	extraData.QuestDescriptionOverrideLocation = currentDatum.QuestDescriptionOverrideLocation
	extraData.NextQuestAllowedCoolDown = currentDatum.NextQuestAllowedCoolDown
	If extraData.NextQuestAllowedCoolDown <= 0.0
		extraData.NextQuestAllowedCoolDown = fDefaultRadiantQuestCoolDownDays
	EndIf
	CurrentRadiantQuestExtraData = extraData
	player.SetValue(PlayerRadiantQuestDataIndex, dataIndex)

	Bool started = False
	Location eventLocation = currentDatum.akLoc
	If !eventLocation
		eventLocation = GetCurrentLocation()
	EndIf
	Int eventValue2 = 0
	If currentDatum.aiValue2_QuestSetting
		eventValue2 = currentDatum.aiValue2_QuestSetting.GetValueInt()
	EndIf
	If currentDatum.StoryEventKeyword
		started = currentDatum.StoryEventKeyword.SendStoryEventAndWait(eventLocation, Self, currentDatum.akRef2, currentDatum.aiValue1, eventValue2)
	EndIf

	COMP_RQ_SpecificAliasesScript specificAliases = currentDatum.akQuestTarget as COMP_RQ_SpecificAliasesScript
	_COMP_RQ_Ins = currentDatum.akQuestTarget as COMP_RQ_Script
	If specificAliases && specificAliases.QuestTarget
		_COMP_RQ_Ins = specificAliases.QuestTarget as COMP_RQ_Script
	EndIf
	If _COMP_RQ_Ins && _COMP_RQ_Ins.IsRunning()
		_COMP_RQ_Ins.Alias_Player.ForceRefIfEmpty(player)
		_COMP_RQ_Ins.Alias_Companion.ForceRefIfEmpty(Self)
	Else
		started = False
	EndIf
	If !started
		player.SetValue(PlayerRadiantQuestDataIndex, fClearedPlayerRadiantQuestDataIndex)
		_COMP_RQ_Ins = None
	EndIf
	Return started
EndFunction

Int Function GetCurrentRadiantFirstObjective()
	Return CurrentRadiantQuestExtraData.CustomFirstObjective
EndFunction

Int Function GetCurrentRadiantSecondObjective()
	Return CurrentRadiantQuestExtraData.CustomSecondObjective
EndFunction

Float Function GetCurrentRadiantQuestCoolDown()
	Return CurrentRadiantQuestExtraData.NextQuestAllowedCoolDown
EndFunction

COMP_RQ_Script Function GetCurrentRadiantQuest()
	If !_COMP_RQ_Ins
		Actor player = Game.GetPlayer()
		If player && RadiantQuestData != None
			Int dataIndex = Math.Floor(player.GetValue(PlayerRadiantQuestDataIndex))
			If dataIndex >= 0 && dataIndex < RadiantQuestData.Length
				Quest selectedQuest = RadiantQuestData[dataIndex].akQuestTarget
				_COMP_RQ_Ins = selectedQuest as COMP_RQ_Script
				COMP_RQ_SpecificAliasesScript specificAliases = selectedQuest as COMP_RQ_SpecificAliasesScript
				If specificAliases && specificAliases.QuestTarget
					_COMP_RQ_Ins = specificAliases.QuestTarget as COMP_RQ_Script
				EndIf
			EndIf
		EndIf
	EndIf
	Return _COMP_RQ_Ins
EndFunction

Function SetRQAcceptanceStage()
	_COMP_RQ_Ins = GetCurrentRadiantQuest()
	If _COMP_RQ_Ins && _COMP_RQ_Ins.IsRunning() && !_COMP_RQ_Ins.IsStageDone(_COMP_RQ_Ins.QuestStageAcceptQuest)
		_COMP_RQ_Ins.SetStage(_COMP_RQ_Ins.QuestStageAcceptQuest)
	EndIf
EndFunction

; WA-03. OutroQuest, OutroQuestStartKeyword and OutroQuestPlayerQuestCount are
; declared and bound on every ally that has a finale and nothing consumed them,
; so no ally outro ever started. The route is the one the radiant starter
; already uses: the outro's own Story Manager node is selected by the start
; keyword and carries its own already-run condition, so a repeat send cannot
; start it twice. Called when a radiant completes, which is the moment the
; player's quest count changes.
Function TryStartOutroQuest()
	If !OutroQuest || !OutroQuestStartKeyword || OutroQuestPlayerQuestCount < 0
		Return
	EndIf
	If OutroQuest.IsRunning() || OutroQuest.IsStarting() || OutroQuest.IsCompleted()
		Return
	EndIf

	Actor player = Game.GetPlayer()
	If !player
		Return
	EndIf
	If player.GetValue(PlayerQuestCountAV) < (OutroQuestPlayerQuestCount as Float)
		Return
	EndIf
	OutroQuestStartKeyword.SendStoryEvent(GetCurrentLocation(), Self, None, 0, 0)
EndFunction
