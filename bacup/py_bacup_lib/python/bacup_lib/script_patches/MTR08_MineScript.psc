Event OnQuestInit()
	bEntranceOpened = False
	bCollapseStarted = False
	bCollapseComplete = False
	bFirstShakeOnce = False
	bRewardTimerActive = False
	bShouldSpawnUltraciteBoss = False
	Quest owner = Self as Quest
	myWave = owner as DefaultQuestEncounterWaveScript
	If myWave != None
		RegisterForCustomEvent(myWave, "LastWave")
	EndIf
	; The converted root has no RunOnStart stage; FO76's event framework set iStartUpStage once the mine was prepared.
	StartTimer(0.5, 4205)
EndEvent

Event OnQuestShutdown()
	CancelMineTimers()
	If myWave != None
		UnregisterForCustomEvent(myWave, "LastWave")
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 4205
		PrepareMine()
	ElseIf aiTimerID == iInitialSpawnID
		StartInitialWave(0)
	ElseIf aiTimerID == 4208
		ForceMineOpen()
	ElseIf aiTimerID == iEndlessWave02StartID || aiTimerID == iEndlessWave03StartID || aiTimerID == iEndlessWave04StartID
		StartEndlessWave(aiTimerID)
	ElseIf aiTimerID == iCentralRewardTimerID
		GrantAutoMinerRewards()
	ElseIf aiTimerID == iAftershockID
		TriggerAftershock()
	ElseIf aiTimerID == iWarningTimerID
		WarnOfCollapse()
	ElseIf aiTimerID == iKillTimerID
		KillPlayerInsideMine()
	ElseIf aiTimerID == iCooldownTimerID
		If IsRunning()
			Stop()
		EndIf
	EndIf
EndEvent

Event DefaultQuestEncounterWaveScript.LastWave(DefaultQuestEncounterWaveScript akSender, Var[] akArgs)
	If akArgs == None || akArgs.Length < 1 || bCollapseStarted
		Return
	EndIf
	; Initial groups spawn deepest first so every enemy is in place before the entrance opens.
	Int waveIndex = akArgs[0] as Int
	If waveIndex == 0 && !IsStageDone(10)
		SetStage(10)
	ElseIf waveIndex == 1 && !IsStageDone(11)
		SetStage(11)
	ElseIf waveIndex == 2 && !IsStageDone(12)
		SetStage(12)
	ElseIf waveIndex == iEntranceWaveIndex && !IsStageDone(13)
		SetStage(13)
	EndIf
EndEvent

Function CancelMineTimers()
	CancelTimer(4205)
	CancelTimer(4208)
	CancelTimer(iInitialSpawnID)
	CancelTimer(iEndlessWave02StartID)
	CancelTimer(iEndlessWave03StartID)
	CancelTimer(iEndlessWave04StartID)
	CancelTimer(iCentralRewardTimerID)
	CancelTimer(iAftershockID)
	CancelTimer(iWarningTimerID)
	CancelTimer(iKillTimerID)
	CancelTimer(iCooldownTimerID)
	bRewardTimerActive = False
EndFunction

Bool Function IsPlayerParticipating()
	Quest owner = Self as Quest
	DefaultEventQuest eventQuest = owner as DefaultEventQuest
	Return eventQuest == None || eventQuest.IsPlayerParticipating()
EndFunction

Function CompleteOpenObjective(Int aiObjective)
	If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
		SetObjectiveCompleted(aiObjective, True)
	EndIf
EndFunction

Function SetCollectionEnabled(RefCollectionAlias akCollection, Bool abEnabled)
	If akCollection == None
		Return
	EndIf
	If abEnabled
		akCollection.EnableAll(False)
	Else
		akCollection.DisableAll(False)
	EndIf
EndFunction

Function StartWave(Int aiWaveIndex)
	If myWave != None && aiWaveIndex >= 0
		myWave.StartEncounterWave(aiWaveIndex)
	EndIf
EndFunction

Function StopWave(Int aiWaveIndex)
	If myWave != None && aiWaveIndex >= 0
		myWave.StopEncounterWave(aiWaveIndex, False)
	EndIf
EndFunction

Function PrepareMine()
	If !IsRunning() || bEntranceOpened
		Return
	EndIf
	If !IsStageDone(iStartUpStage)
		SetStage(iStartUpStage)
	EndIf
	; The placed kill triggers stay armed outside events; keep them off while players are allowed inside.
	SetCollectionEnabled(KillTriggers, False)
	SetCollectionEnabled(EarlyKillTriggers, False)
	SetCollectionEnabled(EntranceBlockers, True)
	CloseRubblePiles()
	EnableMineralVeins()
	PowerDownAutoMiners()
EndFunction

Function EnableMineralVeins()
	If ActiveVeins != None
		ActiveVeins.RemoveAll()
	EndIf
	Int datumIndex = 0
	While MineralData != None && datumIndex < MineralData.Length
		MineralDatum datum = MineralData[datumIndex]
		If datum != None && datum.myCollection != None
			EnableMineralDatum(datum)
		EndIf
		datumIndex += 1
	EndWhile
EndFunction

Function EnableMineralDatum(MineralDatum akDatum)
	RefCollectionAlias veins = akDatum.myCollection
	Int count = veins.GetCount()
	If count <= 0
		Return
	EndIf
	If count > 128
		count = 128
	EndIf
	Int toEnable = akDatum.iFixedNumToEnable
	If toEnable <= 0
		toEnable = Math.Floor(Utility.RandomFloat(akDatum.fMinPercentToEnable, akDatum.fMaxPercentToEnable) * count as Float + 0.5)
	EndIf
	If toEnable > count
		toEnable = count
	EndIf

	Int[] order = New Int[count]
	Int index = 0
	While index < count
		order[index] = index
		index += 1
	EndWhile
	index = 0
	While index < count
		Int swapIndex = Utility.RandomInt(index, count - 1)
		Int held = order[index]
		order[index] = order[swapIndex]
		order[swapIndex] = held
		ObjectReference veinRef = veins.GetAt(order[index])
		If veinRef != None
			If index < toEnable
				veinRef.EnableNoWait(False)
				If ActiveVeins != None
					ActiveVeins.AddRef(veinRef)
				EndIf
				If index == 0 && akDatum.PackageMarker != None
					akDatum.PackageMarker.ForceRefTo(veinRef)
					bShouldSpawnUltraciteBoss = True
				EndIf
			Else
				veinRef.DisableNoWait(False)
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

Function CloseRubblePiles()
	Int index = 0
	While RubblePiles != None && index < RubblePiles.GetCount()
		Default2StateActivator pile = RubblePiles.GetAt(index) as Default2StateActivator
		If pile != None && pile.isOpen
			pile.SetOpenNoWait(False)
		EndIf
		index += 1
	EndWhile
EndFunction

Function OpenRubblePiles(Int aiCount)
	Int opened = 0
	While AvailableRubblePiles != None && opened < aiCount && AvailableRubblePiles.GetCount() > 0
		ObjectReference pileRef = AvailableRubblePiles.GetAt(Utility.RandomInt(0, AvailableRubblePiles.GetCount() - 1))
		AvailableRubblePiles.RemoveRef(pileRef)
		Default2StateActivator pile = pileRef as Default2StateActivator
		If pile != None
			pile.SetOpenNoWait(True)
		EndIf
		opened += 1
	EndWhile
EndFunction

Int Function FindAutoMinerIndex(ReferenceAlias akSourceAlias)
	Int index = 0
	While AutoMinerData != None && index < AutoMinerData.Length
		If AutoMinerData[index] != None && AutoMinerData[index].SourceAlias == akSourceAlias
			Return index
		EndIf
		index += 1
	EndWhile
	Return -1
EndFunction

Function PowerDownAutoMiners()
	Int index = 0
	While AutoMinerData != None && index < AutoMinerData.Length
		AutoMinerDatum datum = AutoMinerData[index]
		If datum != None
			If datum.ActiveAlias != None
				datum.ActiveAlias.Clear()
			EndIf
			If datum.SourceAlias != None && datum.SourceAlias.GetActorReference() != None
				; Restrained rather than downed: a bleeding-out actor cannot be activated for repair.
				datum.SourceAlias.GetActorReference().SetRestrained(True)
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

Function ReleaseAutoMiners()
	Int index = 0
	While AutoMinerData != None && index < AutoMinerData.Length
		AutoMinerDatum datum = AutoMinerData[index]
		If datum != None
			If datum.ActiveAlias != None
				datum.ActiveAlias.Clear()
			EndIf
			If datum.SourceAlias != None && datum.SourceAlias.GetActorReference() != None
				datum.SourceAlias.GetActorReference().SetRestrained(False)
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

Int Function CountActiveAutoMiners()
	Int active = 0
	Int index = 0
	While AutoMinerData != None && index < AutoMinerData.Length
		AutoMinerDatum datum = AutoMinerData[index]
		If datum != None && datum.ActiveAlias != None
			Actor miner = datum.ActiveAlias.GetActorReference()
			If miner != None && !miner.IsDead() && !miner.IsBleedingOut()
				active += 1
			EndIf
		EndIf
		index += 1
	EndWhile
	Return active
EndFunction

Function DisplayAutoMinerObjectives()
	Int index = 0
	While AutoMinerData != None && index < AutoMinerData.Length
		AutoMinerDatum datum = AutoMinerData[index]
		If datum != None && datum.SourceAlias != None && datum.SourceAlias.GetReference() != None
			If datum.ActiveAlias != None && datum.ActiveAlias.GetReference() != None
				SetObjectiveDisplayed(datum.iProtectObjectiveIndex, True, True)
			Else
				SetObjectiveDisplayed(datum.iRepairObjectiveIndex, True, True)
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

Function CloseAutoMinerObjectives()
	Int index = 0
	While AutoMinerData != None && index < AutoMinerData.Length
		AutoMinerDatum datum = AutoMinerData[index]
		If datum != None
			CompleteOpenObjective(datum.iProtectObjectiveIndex)
			If !IsObjectiveCompleted(datum.iRepairObjectiveIndex)
				SetObjectiveDisplayed(datum.iRepairObjectiveIndex, False)
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

Function TryRepairAutoMiner(ReferenceAlias akSourceAlias, ObjectReference akActionRef)
	Actor playerRef = Game.GetPlayer()
	If akActionRef != playerRef || !bEntranceOpened || bCollapseStarted || !IsRunning()
		Return
	EndIf
	Int index = FindAutoMinerIndex(akSourceAlias)
	If index < 0
		Return
	EndIf
	AutoMinerDatum datum = AutoMinerData[index]
	Actor miner = akSourceAlias.GetActorReference()
	If miner == None || miner.IsDead() || datum.ActiveAlias == None || datum.ActiveAlias.GetReference() == miner
		Return
	EndIf
	; co_MTR08_LvlAutoMiner_*_Repair costs 3 c_Steel.
	Component steel = Game.GetFormFromFile(0x0001FABD, "Fallout4.esm") as Component
	If steel != None
		If playerRef.GetComponentCount(steel) < 3
			Return
		EndIf
		playerRef.RemoveComponents(steel, 3, False)
	EndIf
	miner.SetRestrained(False)
	miner.ResetHealthAndLimbs()
	datum.ActiveAlias.ForceRefTo(miner)
	CompleteOpenObjective(datum.iRepairObjectiveIndex)
	SetObjectiveCompleted(datum.iProtectObjectiveIndex, False)
	SetObjectiveDisplayed(datum.iProtectObjectiveIndex, True, True)
	If !bRewardTimerActive
		StartRewardTimer()
	EndIf
EndFunction

Function HandleAutoMinerDown(ReferenceAlias akSourceAlias)
	Int index = FindAutoMinerIndex(akSourceAlias)
	If index < 0
		Return
	EndIf
	AutoMinerDatum datum = AutoMinerData[index]
	Actor miner = akSourceAlias.GetActorReference()
	If datum.ActiveAlias != None && datum.ActiveAlias.GetReference() == miner
		datum.ActiveAlias.Clear()
	EndIf
	If !bEntranceOpened || bCollapseStarted
		Return
	EndIf
	If miner != None
		miner.SetRestrained(True)
	EndIf
	SetObjectiveDisplayed(datum.iProtectObjectiveIndex, False)
	SetObjectiveCompleted(datum.iRepairObjectiveIndex, False)
	SetObjectiveDisplayed(datum.iRepairObjectiveIndex, True, True)
EndFunction

Function StartRewardTimer()
	Float rewardLength = 40.0
	If MTR08_AutoMinerRewardLength != None && MTR08_AutoMinerRewardLength.GetValue() > 0.0
		rewardLength = MTR08_AutoMinerRewardLength.GetValue()
	EndIf
	StartTimer(rewardLength, iCentralRewardTimerID)
	bRewardTimerActive = True
EndFunction

Function GrantAutoMinerRewards()
	bRewardTimerActive = False
	If !bEntranceOpened || bCollapseStarted || !IsRunning()
		Return
	EndIf
	Int active = CountActiveAutoMiners()
	Int tokens = active
	If active > 0 && active >= AutoMinerData.Length && MTR08_AutoMinerJackpotRewardTokenCount != None
		tokens = MTR08_AutoMinerJackpotRewardTokenCount.GetValueInt()
	EndIf
	Actor playerRef = Game.GetPlayer()
	If tokens > 0 && playerRef != None && MTR08_ClaimToken != None && IsPlayerParticipating()
		playerRef.AddItem(MTR08_ClaimToken, tokens, True)
		If MTR08_ClaimToken_HaulDispensedMessage != None
			MTR08_ClaimToken_HaulDispensedMessage.Show(tokens as Float)
		EndIf
	EndIf
	StartRewardTimer()
EndFunction

Function ShakePlayer(Spell akShakeSpell)
	Actor playerRef = Game.GetPlayer()
	If akShakeSpell != None && playerRef != None && IsPlayerParticipating()
		akShakeSpell.Cast(playerRef, playerRef)
	EndIf
EndFunction

Function TriggerAftershock()
	If !bEntranceOpened || bCollapseStarted || !IsRunning()
		Return
	EndIf
	ShakePlayer(MTR08_CameraShakeSpell)
	OpenRubblePiles(Utility.RandomInt(iMinRubblePiles, iMaxRubblePiles))
	bFirstShakeOnce = True
	StartTimer(Utility.RandomInt(iNewAftershockTimerMinLength, iNewAftershockTimerMaxLength) as Float, iAftershockID)
EndFunction

Function WarnOfCollapse()
	If bEntranceOpened && !bCollapseStarted
		ShakePlayer(MTR08_CameraShakeSpellIntense)
	EndIf
EndFunction

Function BeginInitialSpawn()
	If bEntranceOpened || bCollapseStarted
		Return
	EndIf
	ShakePlayer(MTR08_CameraShakeSpell)
	StartTimer(Utility.RandomInt(iInitialSpawnTimerMinLength, iInitialSpawnTimerMaxLength) as Float, iInitialSpawnID)
	; Failsafe: open the mine even if a wave never reports its last subwave.
	StartTimer((iInitialSpawnTimerMaxLength + 60) as Float, 4208)
EndFunction

Function StartInitialWave(Int aiWaveIndex)
	If !bEntranceOpened && !bCollapseStarted
		StartWave(aiWaveIndex)
	EndIf
EndFunction

Function ForceMineOpen()
	If IsRunning() && !bCollapseStarted && !IsStageDone(12)
		SetStage(12)
	EndIf
EndFunction

Function OpenMine()
	If bEntranceOpened || bCollapseStarted
		Return
	EndIf
	bEntranceOpened = True
	CancelTimer(iInitialSpawnID)
	CancelTimer(4208)
	SetCollectionEnabled(EntranceBlockers, False)
	StartWave(iEntranceWaveIndex)
	If bShouldSpawnUltraciteBoss
		StartWave(iBossWWaveIndex)
	EndIf
	DisplayAutoMinerObjectives()
	StartRewardTimer()
	StartTimer(Utility.RandomInt(iFirstAftershockTimerMinLength, iFirstAftershockTimerMaxLength) as Float, iAftershockID)
	If MTR08_InitialCountdownLength != None
		Float warningDelay = MTR08_InitialCountdownLength.GetValue() - iWarningTimerOffset as Float
		If warningDelay > 0.0
			StartTimer(warningDelay, iWarningTimerID)
		EndIf
	EndIf
EndFunction

Function ScheduleEndlessWave(Int aiClearedInitialWaveIndex)
	If bCollapseStarted || !IsRunning()
		Return
	EndIf
	; Stages 20/21/22 report initial waves 2/1/0 wiped out; each group's endless replacement starts after a delay.
	Int timerID = -1
	If aiClearedInitialWaveIndex == 2
		timerID = iEndlessWave02StartID
	ElseIf aiClearedInitialWaveIndex == 1
		timerID = iEndlessWave03StartID
	ElseIf aiClearedInitialWaveIndex == 0
		timerID = iEndlessWave04StartID
	EndIf
	If timerID >= 0
		StartTimer(Utility.RandomInt(iEndlessWaveStartupLengthMin, iEndlessWaveStartupLengthMax) as Float, timerID)
	EndIf
EndFunction

Function StartEndlessWave(Int aiTimerID)
	If bCollapseStarted || !IsRunning()
		Return
	EndIf
	If aiTimerID == iEndlessWave02StartID
		StartWave(iEndlessWave02Index)
	ElseIf aiTimerID == iEndlessWave03StartID
		StartWave(iEndlessWave03Index)
	ElseIf aiTimerID == iEndlessWave04StartID
		StartWave(iEndlessWave04Index)
	EndIf
EndFunction

Function StartAllWavesForDebug()
	Int index = 0
	While index <= iBossWWaveIndex
		StartWave(index)
		index += 1
	EndWhile
EndFunction

Function BeginCollapse()
	If bCollapseStarted
		Return
	EndIf
	bCollapseStarted = True
	CancelMineTimers()
	StopWave(iEndlessWave02Index)
	StopWave(iEndlessWave03Index)
	StopWave(iEndlessWave04Index)
	CloseAutoMinerObjectives()
	ShakePlayer(MTR08_CameraShakeSpellIntense)
	ShakePlayer(MTR08_ApplyCollapseFXSpell)
EndFunction

Function CompleteCollapse()
	If bCollapseComplete
		Return
	EndIf
	If !bCollapseStarted
		BeginCollapse()
	EndIf
	bCollapseComplete = True
	SetCollectionEnabled(EntranceBlockers, True)
	If myWave != None
		myWave.StopAllEncounterWaves(False)
	EndIf
	ReleaseAutoMiners()
	StartTimer(iKillTimerLength as Float, iKillTimerID)
	StartTimer(iCooldownTimerLength as Float, iCooldownTimerID)
EndFunction

Function KillPlayerInsideMine()
	; FO76 kills anyone still inside once the entrance is sealed; checked by cell instead of the placed kill volumes.
	Actor playerRef = Game.GetPlayer()
	If playerRef == None || playerRef.IsDead() || InteriorRootMarker == None
		Return
	EndIf
	ObjectReference interiorRoot = InteriorRootMarker.GetReference()
	If interiorRoot == None
		Return
	EndIf
	Cell mineCell = interiorRoot.GetParentCell()
	If mineCell != None && mineCell.IsInterior() && playerRef.GetParentCell() == mineCell
		playerRef.Kill()
	EndIf
EndFunction

Function ShutdownMine()
	CancelMineTimers()
	If myWave != None
		UnregisterForCustomEvent(myWave, "LastWave")
	EndIf
	SetCollectionEnabled(EntranceBlockers, True)
	SetCollectionEnabled(EarlyKillTriggers, True)
	SetCollectionEnabled(KillTriggers, True)
	ReleaseAutoMiners()
EndFunction
