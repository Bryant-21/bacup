; These FO76 raid announcement topics are unused by the stripped client script
; and do not resolve as Topic objects in the FO4 runtime.
; @drop-property kDifficultyMaxedTopic
; @drop-property kDifficultyIncreasedTopic
; @drop-property kEncounterStartTopic

; FO76 ran this encounter server-side; the client script is declaration-only. Local
; single-player loop per bacup/docs/stub_restoration/contracts/
; rd01-enc02-drill-enc04-squad-2026-09-23.md §2.1. Phase: 0 idle, 1 intro, 2 active,
; 3 outro, 4 finished. The module only sets 9000 (success); 9999 comes from the drill
; alias's DefaultAliasOnDeath. The raid controller owns 10000 and the master stages.

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == 100
		BeginEncounter()
	ElseIf auiStageID == iQuestCompletionStage
		FinishEncounter(SuccessTrack)
	ElseIf auiStageID == 9999
		kEncounterFailedMessage.Show()
		FinishEncounter(FailureTrack)
	ElseIf auiStageID == 10100
		CleanupSpawned()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == iIntroMusicTimerID && Phase == 1
		BeginActivePhase()
	ElseIf aiTimerID == iEnemySpawnTimerID && Phase == 2
		SpawnWave()
		StartTimer(WaveTime.GetValue(), iEnemySpawnTimerID)
	ElseIf aiTimerID == iDifficultyIncrementTimerID && Phase == 2
		IncreaseDifficulty()
	ElseIf aiTimerID == iOutroShakeTimerID && Phase == 3
		ObjectReference explosionMarker = Alias_Marker_Outro_Explosion.GetReference()
		If explosionMarker != None && kOutroExplosion != None
			explosionMarker.PlaceAtMe(kOutroExplosion)
		EndIf
	ElseIf aiTimerID == iOutroRocksTimerID && Phase == 3
		PlayOutroReveal()
	ElseIf aiTimerID == iOutroEndTimerID && Phase == 3
		SetStage(iQuestCompletionStage)
	EndIf
EndEvent

Event RefCollectionAlias.OnActivate(RefCollectionAlias akSender, ObjectReference akSenderRef, ObjectReference akActionRef)
	Actor player = Game.GetPlayer()
	If akSender != Alias_Activators_Fuel || akActionRef != player || Phase < 1 || Phase > 2
		Return
	EndIf
	If player.GetItemCount(kFuelObject) > 0
		kCannotCarryFuelMessage.Show()
		Return
	EndIf
	akSenderRef.Disable()
	ObjectReference canister = akSenderRef.PlaceAtMe(kFuelObject)
	If canister != None
		Alias_MiscItems_Fuel.AddRef(canister)
		player.AddItem(canister, 1, False)
	EndIf
EndEvent

Event ReferenceAlias.OnActivate(ReferenceAlias akSender, ObjectReference akActionRef)
	Actor player = Game.GetPlayer()
	If akSender != Alias_Activator_FuelDepositPoint || akActionRef != player || Phase != 2
		Return
	EndIf
	If player.GetItemCount(kFuelObject) < 1
		kNotEnoughFuelMessage.Show()
		Return
	EndIf
	player.RemoveItem(kFuelObject, 1, True)
	iCurrentFuelDeposited += 1
	Int fuelNeeded = 15
	If NumFuelToDeposit != None && NumFuelToDeposit.GetValueInt() > 0
		fuelNeeded = NumFuelToDeposit.GetValueInt()
	EndIf
	UpdateDrillProgress(fuelNeeded)
	If kFuelDepositedMessage != None
		kFuelDepositedMessage.Show()
	EndIf
	If iCurrentFuelDeposited >= fuelNeeded
		BeginOutro()
	EndIf
EndEvent

Function BeginEncounter()
	CleanupSpawned()
	Phase = 1
	kDrill = Alias_Actor_Drill.GetReference()
	kDrillProgressDummy = Alias_Actor_DrillProgressDummy.GetActorReference()
	If kDrillProgressDummy != None
		kDrillProgressDummy.SetValue(DrillFuelAV, 0.0)
	EndIf
	Alias_Activators_Fuel.EnableAll()
	Alias_Actors_Stalkers.EnableAll()
	RegisterForRemoteEvent(Alias_Activators_Fuel, "OnActivate")
	RegisterForRemoteEvent(Alias_Activator_FuelDepositPoint, "OnActivate")
	IntroTrack.Add()
	StartTimer(IntroTrackLength as Float, iIntroMusicTimerID)
EndFunction

Function BeginActivePhase()
	Phase = 2
	IntroTrack.Remove()
	CombatTrack.Add()
	SpawnWave()
	StartTimer(WaveTime.GetValue(), iEnemySpawnTimerID)
	If fDifficultyTimers != None && fDifficultyTimers.Length > 0
		StartTimer(fDifficultyTimers[0], iDifficultyIncrementTimerID)
	EndIf
EndFunction

Function SpawnWave()
	Int maxRegular = RD01_Enc02_MaxRegularEnemyCount_Global.GetValueInt()
	Int maxSuicider = RD01_Enc02_MaxSuiciderCount_Global.GetValueInt()
	iCurrentRegularEnemyCount = CountLivingEnemies(Alias_Actors_Enemies_Regular)
	iCurrentSuiciderEnemyCount = CountLivingEnemies(Alias_Actors_Enemies_Suiciders)
	Int index = 0
	While kEnemyList != None && index < kEnemyList.Length
		EnemyListDatum entry = kEnemyList[index]
		If entry != None && entry.kActor != None && entry.kSpawnMarker != None && entry.iDifficulty <= iCurrentDifficulty
			ObjectReference marker = entry.kSpawnMarker.GetReference()
			If marker != None
				If kSuiciderEnemyForms.Find(entry.kActor) >= 0
					If iCurrentSuiciderEnemyCount < maxSuicider && SpawnEnemy(marker, entry.kActor, Alias_Actors_Enemies_Suiciders)
						iCurrentSuiciderEnemyCount += 1
					EndIf
				ElseIf iCurrentRegularEnemyCount < maxRegular && SpawnEnemy(marker, entry.kActor, Alias_Actors_Enemies_Regular)
					iCurrentRegularEnemyCount += 1
				EndIf
			EndIf
		EndIf
		index += 1
	EndWhile
	bAtMaxRegularEnemies = iCurrentRegularEnemyCount >= maxRegular
	bAtMaxSuiciderEnemies = iCurrentSuiciderEnemyCount >= maxSuicider
EndFunction

; Refs added after OnAliasInit never get SetPreferredCombatTargets' pass, so each
; spawn is pointed at the drill explicitly.
Bool Function SpawnEnemy(ObjectReference akMarker, ActorBase akBase, RefCollectionAlias akEnemies)
	Actor spawned = akMarker.PlaceActorAtMe(akBase)
	If spawned == None
		Return False
	EndIf
	akEnemies.AddRef(spawned)
	(akEnemies as Quests:_Default:SetPreferredCombatTargets).ApplyPreferredCombatTarget(spawned)
	Return True
EndFunction

Int Function CountLivingEnemies(RefCollectionAlias akEnemies)
	Int living = 0
	Int index = 0
	While index < akEnemies.GetCount()
		Actor enemy = akEnemies.GetAt(index) as Actor
		If enemy != None && !enemy.IsDead()
			living += 1
		EndIf
		index += 1
	EndWhile
	Return living
EndFunction

Function IncreaseDifficulty()
	If iCurrentDifficulty < iMaxDifficulty
		iCurrentDifficulty += 1
		kEnemyPowerUpMessage.Show()
		If iCurrentDifficulty < iMaxDifficulty && fDifficultyTimers != None && iCurrentDifficulty < fDifficultyTimers.Length
			StartTimer(fDifficultyTimers[iCurrentDifficulty], iDifficultyIncrementTimerID)
		EndIf
	EndIf
EndFunction

Function UpdateDrillProgress(Int aiFuelNeeded)
	fFuelIncrementAmount = 100.0 / aiFuelNeeded as Float
	If kDrillProgressDummy != None
		Float progress = iCurrentFuelDeposited as Float * fFuelIncrementAmount
		If progress > 100.0
			progress = 100.0
		EndIf
		kDrillProgressDummy.SetValue(DrillFuelAV, progress)
	EndIf
EndFunction

Function BeginOutro()
	Phase = 3
	CancelTimer(iEnemySpawnTimerID)
	CancelTimer(iDifficultyIncrementTimerID)
	DeleteSpawnedEnemies(Alias_Actors_Enemies_Regular)
	DeleteSpawnedEnemies(Alias_Actors_Enemies_Suiciders)
	Actor player = Game.GetPlayer()
	kCameraShakeSpell.Cast(player, player)
	StartTimer(fOutroShakeWaitTime, iOutroShakeTimerID)
	StartTimer(fOutroRocksWaitTime, iOutroRocksTimerID)
	StartTimer(fOutroEndWaitTime, iOutroEndTimerID)
EndFunction

Function PlayOutroReveal()
	ObjectReference rocks = Alias_MovableStatic_ExitRocks.GetReference()
	If rocks != None
		rocks.Disable()
	EndIf
	Alias_OutroExitRefs.EnableAll()
	Alias_OutroFX.EnableAll()
	ObjectReference drillVisual = Alias_Activator_DrillToAnimate.GetReference()
	If drillVisual != None
		drillVisual.SetOpen(True)
	EndIf
EndFunction

Function FinishEncounter(MusicType akTrack)
	Phase = 4
	CancelEncounterTimers()
	UnregisterForRemoteEvent(Alias_Activators_Fuel, "OnActivate")
	UnregisterForRemoteEvent(Alias_Activator_FuelDepositPoint, "OnActivate")
	DeleteSpawnedEnemies(Alias_Actors_Enemies_Regular)
	DeleteSpawnedEnemies(Alias_Actors_Enemies_Suiciders)
	Alias_Actors_Stalkers.DisableAll()
	IntroTrack.Remove()
	CombatTrack.Remove()
	akTrack.Add()
EndFunction

Function CancelEncounterTimers()
	CancelTimer(iIntroMusicTimerID)
	CancelTimer(iEnemySpawnTimerID)
	CancelTimer(iDifficultyIncrementTimerID)
	CancelTimer(iOutroShakeTimerID)
	CancelTimer(iOutroRocksTimerID)
	CancelTimer(iOutroEndTimerID)
EndFunction

Function DeleteSpawnedEnemies(RefCollectionAlias akEnemies)
	Int index = akEnemies.GetCount() - 1
	While index >= 0
		ObjectReference enemy = akEnemies.GetAt(index)
		If enemy != None
			enemy.Disable()
			enemy.Delete()
		EndIf
		index -= 1
	EndWhile
	akEnemies.RemoveAll()
	bEnemiesDeleted = True
EndFunction

Function RestoreDrill()
	Actor drill = Alias_Actor_Drill.GetActorReference()
	If drill == None
		Return
	EndIf
	If drill.IsDead()
		drill.Resurrect()
	Else
		drill.RestoreValue(Health, drill.GetBaseValue(Health))
	EndIf
EndFunction

; Wipe/retry reset. The raid controller calls this before Stop(); stage 10100
; (RunOnStop) and stage 100 also run it, so the module is clean without Tales.
Function CleanupSpawned()
	Phase = 0
	CancelEncounterTimers()
	UnregisterForRemoteEvent(Alias_Activators_Fuel, "OnActivate")
	UnregisterForRemoteEvent(Alias_Activator_FuelDepositPoint, "OnActivate")
	DeleteSpawnedEnemies(Alias_Actors_Enemies_Regular)
	DeleteSpawnedEnemies(Alias_Actors_Enemies_Suiciders)
	Actor player = Game.GetPlayer()
	Int carried = player.GetItemCount(kFuelObject)
	If carried > 0
		player.RemoveItem(kFuelObject, carried, True)
	EndIf
	Alias_MiscItems_Fuel.RemoveAll()
	Alias_Activators_Fuel.DisableAll()
	Alias_Actors_Stalkers.DisableAll()
	RestoreDrill()
	iCurrentFuelDeposited = 0
	iCurrentDifficulty = 0
	iCurrentRegularEnemyCount = 0
	iCurrentSuiciderEnemyCount = 0
	bAtMaxRegularEnemies = False
	bAtMaxSuiciderEnemies = False
	Actor progressDummy = Alias_Actor_DrillProgressDummy.GetActorReference()
	If progressDummy != None
		progressDummy.SetValue(DrillFuelAV, 0.0)
	EndIf
	IntroTrack.Remove()
	CombatTrack.Remove()
	SuccessTrack.Remove()
	FailureTrack.Remove()
EndFunction
