; RD01 Enc05 Research Lab horde. FO76 ran this encounter server-side; the converted script kept
; only declarations. Contract: bacup/docs/stub_restoration/contracts/rd01-enc01-guardian-enc05-lab-2026-09-23.md §5.
; Converted RD01 scripts never call Tales. Tales reads the horde progress as the Health percentage
; of Alias_Actor_HordeDummy (alias 9); solo tuning reaches this script only through the
; MaxEnemyCount (78DFA3) and WaveTime (7AC17E) globals. Success sets iQuestCompletionStage once;
; the raid controller owns stage 10000 and the master owner stage.
; Phase: 0 idle, 1 fighting, 2 won. Extra timer id: 51 outro.

Event OnQuestShutdown()
	CleanupSpawned()
EndEvent

Function StartEncounter()
	CleanupSpawned()
	HordeDummy = Alias_Actor_HordeDummy.GetActorReference()
	fTotalEnemyCount = kEnemyList.Length as Float
	fHPBarReductionAmount = 0.0
	If HordeDummy != None && fTotalEnemyCount > 0.0
		fHPBarReductionAmount = HordeDummy.GetBaseValue(Health) / fTotalEnemyCount
	EndIf
	Phase = 1
	iCurrentWave = 0

	IntroTrack.Add()
	If HordeDummy != None
		HordeDummy.Say(EncounterStartTopic)
	EndIf
	StartTimer(IntroTrackLength as Float, iIntroMusicTimerID)
	SpawnCurrentWave()
	StartTimer(WaveDuration(), iWaveTimerID)
EndFunction

Function SpawnCurrentWave()
	Int maxEnemies = 20
	If RD01_Enc05_MaxEnemyCount_Global != None && RD01_Enc05_MaxEnemyCount_Global.GetValueInt() > 0
		maxEnemies = RD01_Enc05_MaxEnemyCount_Global.GetValueInt()
	EndIf
	bAtMaxEnemies = False
	Actor player = Game.GetPlayer()
	While iPausedWaveIndex < kEnemyList.Length && Phase == 1
		EnemyListDatum entry = kEnemyList[iPausedWaveIndex]
		If entry != None && entry.iWave == iCurrentWave
			If iCurrentEnemyCount >= maxEnemies
				bAtMaxEnemies = True
				Return
			EndIf
			Actor spawned = None
			ObjectReference marker = None
			If entry.kSpawnMarker != None
				marker = entry.kSpawnMarker.GetReference()
			EndIf
			If marker != None && entry.kActor != None
				spawned = marker.PlaceActorAtMe(entry.kActor)
			EndIf
			If spawned != None
				iCurrentEnemyCount += 1
				Alias_Actors_Enemies.AddRef(spawned)
				RegisterForRemoteEvent(spawned, "OnDeath")
				spawned.StartCombat(player)
			Else
				; An entry that cannot spawn still counts, or the horde bar could never empty.
				CountKill()
			EndIf
		EndIf
		iPausedWaveIndex += 1
	EndWhile
EndFunction

Bool Function CurrentWaveFullySpawned()
	Return iPausedWaveIndex >= kEnemyList.Length && !bAtMaxEnemies
EndFunction

Int Function LastWave()
	Int last = iTotalWaves
	Int index = 0
	While index < kEnemyList.Length
		If kEnemyList[index] != None && kEnemyList[index].iWave > last
			last = kEnemyList[index].iWave
		EndIf
		index += 1
	EndWhile
	Return last
EndFunction

Function AdvanceWave()
	If Phase != 1 || !CurrentWaveFullySpawned() || iCurrentWave >= LastWave()
		Return
	EndIf
	iCurrentWave += 1
	iPausedWaveIndex = 0
	SpawnCurrentWave()
	StartTimer(WaveDuration(), iWaveTimerID)
EndFunction

Event Actor.OnDeath(Actor akSender, Actor akKiller)
	UnregisterForRemoteEvent(akSender, "OnDeath")
	If Phase != 1 || Alias_Actors_Enemies.Find(akSender) < 0
		Return
	EndIf
	iCurrentEnemyCount -= 1
	CountKill()
	If Phase != 1
		Return
	EndIf
	If bAtMaxEnemies
		SpawnCurrentWave()
	EndIf
	If iCurrentEnemyCount <= 0 && CurrentWaveFullySpawned()
		AdvanceWave()
	EndIf
EndEvent

Function CountKill()
	iEnemyKillCount += 1
	If HordeDummy != None && fHPBarReductionAmount > 0.0 && !HordeDummy.IsDead()
		HordeDummy.DamageValue(Health, fHPBarReductionAmount)
	EndIf
	If iEnemyKillCount as Float >= fTotalEnemyCount && Phase == 1
		SetStage(iQuestCompletionStage)
	EndIf
EndFunction

Function EncounterVictory()
	If Phase != 1
		Return
	EndIf
	Phase = 2
	CancelTimer(iWaveTimerID)
	CancelTimer(iIntroMusicTimerID)
	IntroTrack.Remove()
	CombatTrack.Remove()
	SuccessTrack.Add()
	If HordeDummy != None
		HordeDummy.Say(EncounterEndTopic)
	EndIf
	StartTimer(OutroLength, 51)
EndFunction

; Wipe-reset entry point the raid controller calls on every module (also run on shutdown and at
; every start). Deletes the horde, refills the progress dummy and restores every crystal; the
; controller unseals the doors.
Function CleanupSpawned()
	Phase = 0
	CancelTimer(iWaveTimerID)
	CancelTimer(iIntroMusicTimerID)
	CancelTimer(51)
	IntroTrack.Remove()
	CombatTrack.Remove()
	SuccessTrack.Remove()

	Int index = 0
	While index < Alias_Actors_Enemies.GetCount()
		ObjectReference enemy = Alias_Actors_Enemies.GetAt(index)
		If enemy != None
			UnregisterForRemoteEvent(enemy as Actor, "OnDeath")
			enemy.Disable()
			enemy.Delete()
		EndIf
		index += 1
	EndWhile
	Alias_Actors_Enemies.RemoveAll()

	If HordeDummy != None
		If HordeDummy.IsDead()
			HordeDummy.Resurrect()
		EndIf
		HordeDummy.RestoreValue(Health, HordeDummy.GetBaseValue(Health))
	EndIf

	index = 0
	While index < Alias_Crystals.GetCount()
		ObjectReference crystal = Alias_Crystals.GetAt(index)
		If crystal != None
			crystal.ClearDestruction()
			crystal.Enable()
		EndIf
		index += 1
	EndWhile

	iCurrentWave = 0
	iPausedWaveIndex = 0
	iCurrentEnemyCount = 0
	iEnemyKillCount = 0
	bAtMaxEnemies = False
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID == 51
		SuccessTrack.Remove()
	ElseIf aiTimerID == iIntroMusicTimerID
		IntroTrack.Remove()
		If Phase == 1
			CombatTrack.Add()
		EndIf
	ElseIf aiTimerID == iWaveTimerID && Phase == 1
		If bAtMaxEnemies
			SpawnCurrentWave()
		EndIf
		If CurrentWaveFullySpawned()
			AdvanceWave()
		Else
			StartTimer(WaveDuration(), iWaveTimerID)
		EndIf
	EndIf
EndEvent

Float Function WaveDuration()
	If WaveTime != None && WaveTime.GetValue() > 0.0
		Return WaveTime.GetValue()
	EndIf
	Return 30.0
EndFunction
