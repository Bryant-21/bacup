E09B_MobWaves Function WaveScript()
	Quest owner = Self as Quest
	Return owner as E09B_MobWaves
EndFunction

Bool Function PlayerParticipating()
	Quest owner = Self as Quest
	DefaultEventQuest eventQuest = owner as DefaultEventQuest
	Return eventQuest == None || eventQuest.IsPlayerParticipating()
EndFunction

Function SetQuestVariable(String asName, Float afValue)
	Quest owner = Self as Quest
	B21:QuestVariables questVariables = owner as B21:QuestVariables
	If questVariables != None
		questVariables.SetVariable(asName, afValue)
	EndIf
EndFunction

Function SetReferenceEnabled(ReferenceAlias akAlias, Bool abEnabled)
	If akAlias == None || akAlias.GetReference() == None
		Return
	EndIf
	If abEnabled
		akAlias.GetReference().Enable(False)
	Else
		akAlias.GetReference().Disable(False)
	EndIf
EndFunction

Function SetButtonState(String asState)
	If Alias_Button == None
		Return
	EndIf
	E09B_ButtonAnimationScript button = Alias_Button.GetReference() as E09B_ButtonAnimationScript
	If button != None
		button.GoToState(asState)
	EndIf
EndFunction

Function SetRoundLights(Int aiWheelResult)
	; Wheel results: 0 red boss beatdown, 1 blue crowd control, 2 yellow twist.
	SetReferenceEnabled(Alias_RedLightParent, aiWheelResult == 0)
	SetReferenceEnabled(Alias_BlueLightParent, aiWheelResult == 1)
	SetReferenceEnabled(Alias_YellowLightParent, aiWheelResult == 2)
EndFunction

Function BuildAvailableRounds()
	; EncounterWaves lists the bossCount boss waves first, then the mobCount crowd-control waves.
	availableBosses = New Int[0]
	Int index = 0
	While index < bossCount
		availableBosses.Add(index)
		index += 1
	EndWhile
	availableMobs = New Int[0]
	index = 0
	While index < mobCount
		availableMobs.Add(bossCount + index)
		index += 1
	EndWhile

	availableTwists = New TwistData[0]
	twistNames = New Location[0]
	index = 0
	While twistStages != None && index < twistStages.Length
		TwistData twist = New TwistData
		twist.stageIndex = twistStages[index]
		If twistNameList != None && index < twistNameList.GetSize()
			twist.waveName = twistNameList.GetAt(index) as Location
		EndIf
		availableTwists.Add(twist)
		twistNames.Add(twist.waveName)
		index += 1
	EndWhile
EndFunction

Function ResetImposter()
	Int index = 0
	While ImposterCappySpawnPoints != None && index < ImposterCappySpawnPoints.Length
		If ImposterCappySpawnPoints[index] != None
			ImposterCappySpawnPoints[index].MoveToMyEditorLocation()
		EndIf
		index += 1
	EndWhile
	If ImposterCappyRef != None
		ImposterCappyRef.MoveToMyEditorLocation()
	EndIf
EndFunction

Function ResetGameState()
	CancelTimer(iExplosionTimerID)
	CancelTimer(11)
	CancelTimer(12)
	CancelTimer(13)
	spins = 0
	canSpin = False
	; True means no round is open, so a stray Post Spin stage before the first spin does nothing.
	alreadySetPostSpinStageThisSpin = True
	wheelResult = -1
	targetStage = -1
	fightResult = -1
	twistPicked = -1
	twistStagePicked = -1
	lastTwistPicked = -1
	lastBossFightResult = -1
	lastMobFightResult = -1
	cappysFound = 0
	cappys = New ObjectReference[0]
	B21WavesFailed = 0
	B21CappyHuntTotal = 0
	B21ChickensRemaining = 0
	B21CaughtChickens = New ObjectReference[0]
	B21BrahminTippingActive = False
	B21TippedBrahmin = New ObjectReference[0]
	B21KeepMovingActive = False
	B21KeepMovingExplosions = 0
	B21IntroScene = None
	enemyWaves = WaveScript()
	BuildAvailableRounds()
	SetRoundLights(-1)
	SetButtonState("inactive")
	ResetImposter()
	SetQuestVariable("wave", 0.0)
	SetQuestVariable("WavesFailed", 0.0)
	SetQuestVariable("DefencePointsDestroyed", 0.0)
	SetQuestVariable("CappysFound", 0.0)
	SetQuestVariable("CappysToFind", CappysToSpawn as Float)
	SetQuestVariable("BrahminPushed", 0.0)
EndFunction

Function ArmIntroFallback(Scene akIntroScene)
	B21IntroScene = akIntroScene
	StartTimer(30.0, 12)
EndFunction

Function CheckIntroFinished()
	If !IsRunning() || IsStageDone(spinStage) || IsStageDone(9999)
		Return
	EndIf
	If B21IntroScene != None && B21IntroScene.IsPlaying()
		StartTimer(5.0, 12)
		Return
	EndIf
	; The intro's final line sets the spin stage; a scene that never ran must not strand the rules objective.
	SetStage(spinStage)
EndFunction

Int Function CurrentSpinObjective()
	If spinObjectiveIDs == None || spins < 0 || spins >= spinObjectiveIDs.Length
		Return -1
	EndIf
	Return spinObjectiveIDs[spins]
EndFunction

Bool Function HasSpinsRemaining()
	Return spinObjectiveIDs != None && spins < spinObjectiveIDs.Length
EndFunction

Bool Function IsRoundOpen()
	Return !alreadySetPostSpinStageThisSpin
EndFunction

Int Function CurrentWheelResult()
	Return wheelResult
EndFunction

Int Function CurrentTwistStage()
	Return twistStagePicked
EndFunction

Function OpenSpinPrompt()
	CancelTimer(12)
	canSpin = True
	SetButtonState("active")
EndFunction

Bool Function TryBeginSpin()
	If !canSpin || !IsRunning() || IsStageDone(9999) || IsStageDone(finishStage)
		Return False
	EndIf
	canSpin = False
	Return True
EndFunction

Int Function PickWheelResult()
	Int[] choices = New Int[0]
	If availableBosses != None && availableBosses.Length > 0
		choices.Add(0)
	EndIf
	If availableMobs != None && availableMobs.Length > 0
		choices.Add(1)
	EndIf
	If availableTwists != None && availableTwists.Length > 0
		choices.Add(2)
	EndIf
	If choices.Length == 0
		Return -1
	EndIf
	Return choices[Utility.RandomInt(0, choices.Length - 1)]
EndFunction

Int Function WheelAnimationIndex(E09B_Wheel_Script akWheel, Int aiWheelResult)
	If aiWheelResult == 2
		Return akWheel.yellowAnimationIndex
	EndIf
	Int[] indices = akWheel.blueAnimationIndices
	If aiWheelResult == 0
		indices = akWheel.redAnimationIndices
	EndIf
	If indices == None || indices.Length == 0
		Return 0
	EndIf
	Return indices[Utility.RandomInt(0, indices.Length - 1)]
EndFunction

Function SpinWheel()
	canSpin = False
	SetButtonState("inactive")
	spins += 1
	SetQuestVariable("wave", spins as Float)
	alreadySetPostSpinStageThisSpin = False
	wheelResult = PickWheelResult()
	If wheelResult == 0
		targetStage = bossStage
	ElseIf wheelResult == 1
		targetStage = fightStage
	ElseIf wheelResult == 2
		targetStage = twistStage
	Else
		targetStage = -1
	EndIf

	If Alias_Wheel != None
		wheelScript = Alias_Wheel.GetReference() as E09B_Wheel_Script
	EndIf
	If wheelScript != None && wheelResult >= 0
		wheelScript.PlayWheelSpinAnimation(WheelAnimationIndex(wheelScript, wheelResult))
	EndIf
	Float delay = spinAnimationTime
	If delay < 1.0
		delay = 1.0
	EndIf
	StartTimer(delay, 11)
EndFunction

Function RevealSpinResult()
	If !IsRunning() || IsStageDone(9999) || !IsRoundOpen()
		Return
	EndIf
	If targetStage < 0
		SetStage(5000)
		Return
	EndIf
	SetRoundLights(wheelResult)
	Int stage = targetStage
	targetStage = -1
	SetStage(stage)
EndFunction

Int Function TakeRandomEntry(Int[] akPool)
	If akPool == None || akPool.Length == 0
		Return -1
	EndIf
	Int pick = Utility.RandomInt(0, akPool.Length - 1)
	Int value = akPool[pick]
	akPool.Remove(pick)
	Return value
EndFunction

Function StartCombatRound(Bool abBoss)
	Int waveIndex = -1
	If abBoss
		waveIndex = TakeRandomEntry(availableBosses)
		lastBossFightResult = waveIndex
	Else
		waveIndex = TakeRandomEntry(availableMobs)
		lastMobFightResult = waveIndex
	EndIf
	fightResult = waveIndex
	E09B_MobWaves waves = WaveScript()
	If waves == None || waveIndex < 0
		Return
	EndIf
	Location waveName = waves.WaveNameLocation(waveIndex)
	If waveName != None
		If abBoss && bossName_Location != None
			bossName_Location.ForceLocationTo(waveName)
		ElseIf !abBoss && mobName_Location != None
			mobName_Location.ForceLocationTo(waveName)
		EndIf
	EndIf
	waves.StartWave(waveIndex)
EndFunction

Int Function StartTwistRound()
	If availableTwists == None || availableTwists.Length == 0
		Return -1
	EndIf
	Int pick = Utility.RandomInt(0, availableTwists.Length - 1)
	TwistData twist = availableTwists[pick]
	availableTwists.Remove(pick)
	If twist == None
		Return -1
	EndIf
	twistStagePicked = twist.stageIndex
	If twistStages != None
		twistPicked = twistStages.Find(twist.stageIndex)
	EndIf
	lastTwistPicked = twistPicked
	If twist.waveName != None && twistName_Location != None
		twistName_Location.ForceLocationTo(twist.waveName)
	EndIf
	Return twist.stageIndex
EndFunction

Bool Function TryClosePostSpin()
	If alreadySetPostSpinStageThisSpin
		Return False
	EndIf
	alreadySetPostSpinStageThisSpin = True
	CancelTimer(11)
	Return True
EndFunction

Function FinishRound(Bool abSucceeded)
	SetRoundLights(-1)
	If !abSucceeded
		B21WavesFailed += 1
		SetQuestVariable("WavesFailed", B21WavesFailed as Float)
	EndIf
EndFunction

Function BeginImposter()
	ResetImposter()
	If ImposterCappyRef == None || ImposterCappySpawnPoints == None || ImposterCappySpawnPoints.Length == 0
		Return
	EndIf
	ObjectReference decoy = ImposterCappySpawnPoints[Utility.RandomInt(0, ImposterCappySpawnPoints.Length - 1)]
	If decoy == None
		Return
	EndIf
	; The imposter takes a real Cappy's place; that Cappy waits at the holding marker until the round resets.
	ImposterCappyRef.MoveTo(decoy)
	If ImposterCappyMarker != None
		decoy.MoveTo(ImposterCappyMarker)
	EndIf
EndFunction

Function BeginCappyHunt(Int aiAvailableCappys)
	cappys = New ObjectReference[0]
	cappysFound = 0
	B21CappyHuntTotal = aiAvailableCappys
	If CappysToSpawn > 0 && CappysToSpawn < B21CappyHuntTotal
		B21CappyHuntTotal = CappysToSpawn
	EndIf
	SetQuestVariable("CappysFound", 0.0)
	SetQuestVariable("CappysToFind", B21CappyHuntTotal as Float)
EndFunction

Function HandleCappyFound(ObjectReference akCappy)
	If B21CappyHuntTotal <= 0 || akCappy == None || cappys == None || cappys.Find(akCappy) >= 0
		Return
	EndIf
	cappys.Add(akCappy)
	cappysFound = cappys.Length
	akCappy.DisableNoWait(True)
	SetQuestVariable("CappysFound", cappysFound as Float)
	If cappysFound >= B21CappyHuntTotal
		B21CappyHuntTotal = 0
		If allCappysFoundStage >= 0
			SetStage(allCappysFoundStage)
		EndIf
	EndIf
EndFunction

Function EndCappyHunt()
	B21CappyHuntTotal = 0
EndFunction

Function BeginChickenChase(Int aiChickenCount)
	B21CaughtChickens = New ObjectReference[0]
	B21ChickensRemaining = aiChickenCount
EndFunction

Function HandleChickenCaught(ObjectReference akChicken)
	If B21ChickensRemaining <= 0 || akChicken == None || B21CaughtChickens == None || B21CaughtChickens.Find(akChicken) >= 0
		Return
	EndIf
	B21CaughtChickens.Add(akChicken)
	B21ChickensRemaining -= 1
	If B21ChickensRemaining <= 0
		; 4115 is TwistChickenChaseFinished, the same stage objective 305's timer sets.
		SetStage(4115)
	EndIf
EndFunction

Bool Function EndChickenChase()
	Bool caughtAll = B21ChickensRemaining <= 0
	B21ChickensRemaining = 0
	Return caughtAll
EndFunction

Function BeginBrahminTipping()
	B21TippedBrahmin = New ObjectReference[0]
	B21BrahminTippingActive = True
	SetQuestVariable("BrahminPushed", 0.0)
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && brahminTippingPerk != None && !playerRef.HasPerk(brahminTippingPerk)
		playerRef.AddPerk(brahminTippingPerk)
	EndIf
EndFunction

Function HandleWaveActorAdded(Actor akActor, Int aiWaveIndex)
	If !B21BrahminTippingActive || akActor == None
		Return
	EndIf
	RegisterForRemoteEvent(akActor, "OnActivate")
	RegisterForHitEvent(akActor, Game.GetPlayer())
EndFunction

Function TipBrahmin(Actor akBrahmin)
	If !B21BrahminTippingActive || akBrahmin == None || akBrahmin.IsDead() || B21TippedBrahmin.Find(akBrahmin) >= 0
		Return
	EndIf
	Actor playerRef = Game.GetPlayer()
	B21TippedBrahmin.Add(akBrahmin)
	UnregisterForRemoteEvent(akBrahmin, "OnActivate")
	If playerRef != None
		; E09B_TippableBrahminServer's default impulse.
		playerRef.PushActorAway(akBrahmin, 10.0)
	EndIf
	SetQuestVariable("BrahminPushed", B21TippedBrahmin.Length as Float)
	E09B_MobWaves waves = WaveScript()
	If waves != None && !waves.IsWaveSpawning() && waves.CountLivingActorsNotIn(B21TippedBrahmin) <= 0
		; 4095 is TwistBrahminTippingDone, the same stage objective 320's timer sets.
		SetStage(4095)
	EndIf
EndFunction

Bool Function EndBrahminTipping()
	B21BrahminTippingActive = False
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && brahminTippingPerk != None && playerRef.HasPerk(brahminTippingPerk)
		playerRef.RemovePerk(brahminTippingPerk)
	EndIf
	E09B_MobWaves waves = WaveScript()
	Return waves == None || waves.CountLivingActorsNotIn(B21TippedBrahmin) <= 0
EndFunction

Function RecordPlayerPosition()
	Actor playerRef = Game.GetPlayer()
	If playerRef == None
		Return
	EndIf
	B21KeepMovingLastX = playerRef.GetPositionX()
	B21KeepMovingLastY = playerRef.GetPositionY()
	B21KeepMovingLastZ = playerRef.GetPositionZ()
EndFunction

Float Function ExplosionInterval()
	If fTimeBetweenExplosions > 0.0
		Return fTimeBetweenExplosions
	EndIf
	Return 5.0
EndFunction

Function BeginKeepMoving()
	B21KeepMovingExplosions = 0
	B21KeepMovingActive = True
	RecordPlayerPosition()
	StartTimer(ExplosionInterval(), iExplosionTimerID)
EndFunction

Function TickKeepMoving()
	If !B21KeepMovingActive || !IsRunning()
		Return
	EndIf
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && PlayerParticipating()
		Float dx = playerRef.GetPositionX() - B21KeepMovingLastX
		Float dy = playerRef.GetPositionY() - B21KeepMovingLastY
		Float dz = playerRef.GetPositionZ() - B21KeepMovingLastZ
		; Less than 64 units of travel since the last check counts as standing still.
		If dx * dx + dy * dy + dz * dz < 4096.0
			If ExplosionToPlaceAtPlayers != None && (iDeathsPerPlayer <= 0 || B21KeepMovingExplosions < iDeathsPerPlayer)
				playerRef.PlaceAtMe(ExplosionToPlaceAtPlayers)
			EndIf
			B21KeepMovingExplosions += 1
		EndIf
		RecordPlayerPosition()
	EndIf
	StartTimer(ExplosionInterval(), iExplosionTimerID)
EndFunction

Bool Function EndKeepMoving()
	B21KeepMovingActive = False
	CancelTimer(iExplosionTimerID)
	Return iDeathsPerPlayer <= 0 || B21KeepMovingExplosions < iDeathsPerPlayer
EndFunction

Function ScheduleShutdown(Float afDelay)
	StartTimer(afDelay, 13)
EndFunction

Function CleanupGame()
	CancelTimer(11)
	CancelTimer(12)
	canSpin = False
	B21CappyHuntTotal = 0
	B21ChickensRemaining = 0
	EndKeepMoving()
	EndBrahminTipping()
	SetRoundLights(-1)
	SetButtonState("inactive")
	ResetImposter()
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID == 11
		RevealSpinResult()
	ElseIf aiTimerID == 12
		CheckIntroFinished()
	ElseIf aiTimerID == 13
		If IsRunning()
			Stop()
		EndIf
	ElseIf aiTimerID == iExplosionTimerID
		TickKeepMoving()
	EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer()
		TipBrahmin(akSender as Actor)
	EndIf
EndEvent

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
	If !B21BrahminTippingActive || akTarget == None
		Return
	EndIf
	If akAggressor == Game.GetPlayer() && akProjectile == None
		TipBrahmin(akTarget as Actor)
	Else
		RegisterForHitEvent(akTarget, Game.GetPlayer())
	EndIf
EndEvent

Event OnQuestShutdown()
	CancelTimer(13)
	CleanupGame()
EndEvent
