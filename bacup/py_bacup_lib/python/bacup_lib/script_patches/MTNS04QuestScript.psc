DefaultQuestEncounterWaveScript Function WaveScript()
	Quest owner = Self as Quest
	Return owner as DefaultQuestEncounterWaveScript
EndFunction

Bool Function IsAggroActive()
	Return bAggroStart && IsRunning() && !IsStageDone(WendigoFightStage)
EndFunction

Bool Function IsPlayerParticipating()
	Quest owner = Self as Quest
	DefaultEventQuest eventQuest = owner as DefaultEventQuest
	Return eventQuest == None || eventQuest.IsPlayerParticipating()
EndFunction

Default2StateActivator Function JukeboxScript()
	If MTNS04_Jukebox_Ref != None
		Return MTNS04_Jukebox_Ref
	EndIf
	If Jukebox != None && Jukebox.GetReference() != None
		Return Jukebox.GetReference() as Default2StateActivator
	EndIf
	Return None
EndFunction

Bool Function IsJukeboxPlaying()
	Default2StateActivator jukeboxRef = JukeboxScript()
	Return jukeboxRef != None && jukeboxRef.IsOpen && !jukeboxRef.IsDestroyed()
EndFunction

Bool Function IsPlayerDrunk(Actor akPlayer)
	Return akPlayer != None && AlcoholEffect != None && akPlayer.HasMagicEffectWithKeyword(AlcoholEffect)
EndFunction

Bool Function IsInstrumentWeapon(Weapon akWeapon)
	If akWeapon == None
		Return False
	EndIf
	Return (WeaponTypeInstrument != None && akWeapon.HasKeyword(WeaponTypeInstrument)) || (ma_WarDrum != None && akWeapon.HasKeyword(ma_WarDrum))
EndFunction

Function ResetActivity()
	CancelTimer(MusicAggroTimerId)
	CancelTimer(WaveActiveTimerID)
	CancelTimer(WavePauseTimerID)
	CancelTimer(7)
	bAggroStart = False
	EndlessWavePaused = False
	iCurrentAggro = 0
	fAggroProgressPercent = 0.0
	bAggroThreshold01 = False
	bAggroThreshold02 = False
	bAggroThreshold03 = False
	bWendigoDefeatedUnarmed = True
	Int required = iAggroRequiredTotal
	If required <= 0
		required = 2800
	EndIf
	iAggroThreshold01 = required / 4
	iAggroThreshold02 = required / 2
	iAggroThreshold03 = (required * 3) / 4
EndFunction

Function SayHowl(Topic akHowl)
	Actor playerRef = Game.GetPlayer()
	If akHowl != None && playerRef != None
		playerRef.Say(akHowl, None, True)
	EndIf
EndFunction

Function ReportAggroThreshold(Topic akHowl)
	SayHowl(akHowl)
	If MTNS04_AggroTotalMessage != None
		MTNS04_AggroTotalMessage.Show(fAggroProgressPercent)
	EndIf
EndFunction

Function AddAggro(Int aiAmount)
	If aiAmount <= 0 || !IsAggroActive()
		Return
	EndIf
	iCurrentAggro += aiAmount
	Int required = iAggroRequiredTotal
	If required <= 0
		required = 2800
	EndIf
	fAggroProgressPercent = (iCurrentAggro as Float) * 100.0 / (required as Float)
	If fAggroProgressPercent > 100.0
		fAggroProgressPercent = 100.0
	EndIf

	If !bAggroThreshold01 && iCurrentAggro >= iAggroThreshold01
		bAggroThreshold01 = True
		ReportAggroThreshold(MTNS04_Night_WendigoHowlFar)
	EndIf
	If !bAggroThreshold02 && iCurrentAggro >= iAggroThreshold02
		bAggroThreshold02 = True
		ReportAggroThreshold(MTNS04_Night_WendigoHowlMed)
	EndIf
	If !bAggroThreshold03 && iCurrentAggro >= iAggroThreshold03
		bAggroThreshold03 = True
		ReportAggroThreshold(MTNS04_Night_WendigoHowlNear)
	EndIf
	If iCurrentAggro >= required && !IsStageDone(WendigoFightStage)
		SetStage(WendigoFightStage)
	EndIf
EndFunction

Function AddInstrumentAggro(Actor akPlayer)
	If akPlayer == None || akPlayer != Game.GetPlayer() || !IsPlayerParticipating()
		Return
	EndIf
	If IsPlayerDrunk(akPlayer)
		AddAggro(iAggroInstrumentAmount + iAggroInstrumentDrunkAmount)
	Else
		AddAggro(iAggroInstrumentAmount)
	EndIf
EndFunction

Function AddWeaponAggro(Actor akAttacker, Weapon akWeapon, Int aiAmount)
	If akAttacker == None || akAttacker != Game.GetPlayer() || !IsInstrumentWeapon(akWeapon)
		Return
	EndIf
	AddAggro(aiAmount)
EndFunction

Function TickAggro()
	If !IsAggroActive()
		Return
	EndIf
	Int amount = 0
	If IsJukeboxPlaying()
		amount += iAggroJukeboxMusicAmount
	EndIf
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && IsPlayerParticipating()
		; Watching the swing event again each tick survives load and 3D resets.
		RegisterForAnimationEvent(playerRef, "weaponSwing")
		If IsPlayerDrunk(playerRef)
			amount += iAggroDrunkAmount
		EndIf
	EndIf
	AddAggro(amount)
EndFunction

Function SetGhoulWaveActive(Bool abActive)
	DefaultQuestEncounterWaveScript waves = WaveScript()
	If waves == None
		Return
	EndIf
	Int ghoulWave = waves.FindEncounterWaveIndex("Ghouls")
	If ghoulWave < 0
		Return
	EndIf
	If abActive
		EndlessWavePaused = False
		waves.StartEncounterWave(ghoulWave)
		StartTimer(WaveActiveTimerLength, WaveActiveTimerID)
	Else
		EndlessWavePaused = True
		waves.StopEncounterWave(ghoulWave, False)
		StartTimer(WavePauseTimerLength, WavePauseTimerID)
	EndIf
EndFunction

Function StartAggroActivity()
	If bAggroStart || IsStageDone(WendigoFightStage)
		Return
	EndIf
	bAggroStart = True
	Actor playerRef = Game.GetPlayer()
	If LightEnableMarker != None && LightEnableMarker.GetReference() != None
		LightEnableMarker.GetReference().Enable(False)
	EndIf
	If playerRef != None
		RegisterForAnimationEvent(playerRef, "weaponSwing")
	EndIf
	SetGhoulWaveActive(True)
	StartTimer(MusicAggroTimerLength as Float, MusicAggroTimerId)
EndFunction

Function StartWendigoFight()
	CancelTimer(MusicAggroTimerId)
	fAggroProgressPercent = 100.0
	DefaultQuestEncounterWaveScript waves = WaveScript()
	If waves != None
		Int bossWave = waves.FindEncounterWaveIndex("Wendigo Boss")
		If bossWave >= 0
			waves.StartEncounterWave(bossWave)
		EndIf
	EndIf
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && MTNS04NightstalkerAppearImodSpell != None
		MTNS04NightstalkerAppearImodSpell.Cast(playerRef, playerRef)
	EndIf
	SayHowl(MTNS04_Night_WendigoHowlNear)
EndFunction

Function EndActivity()
	bAggroStart = False
	CancelTimer(MusicAggroTimerId)
	CancelTimer(WaveActiveTimerID)
	CancelTimer(WavePauseTimerID)
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		UnregisterForAnimationEvent(playerRef, "weaponSwing")
	EndIf
	DefaultQuestEncounterWaveScript waves = WaveScript()
	If waves != None
		waves.StopAllEncounterWaves(False)
	EndIf
	StartTimer(15.0, 7)
EndFunction

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
	Actor playerRef = Game.GetPlayer()
	If akSource == playerRef && asEventName == "weaponSwing" && playerRef != None
		AddWeaponAggro(playerRef, playerRef.GetEquippedWeapon(), iAggroWeaponSwingAmount)
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == MusicAggroTimerId
		If IsAggroActive()
			TickAggro()
			StartTimer(MusicAggroTimerLength as Float, MusicAggroTimerId)
		EndIf
	ElseIf aiTimerID == WaveActiveTimerID
		If bAggroStart && IsRunning() && !IsStageDone(400)
			SetGhoulWaveActive(False)
		EndIf
	ElseIf aiTimerID == WavePauseTimerID
		If bAggroStart && IsRunning() && !IsStageDone(400)
			SetGhoulWaveActive(True)
		EndIf
	ElseIf aiTimerID == 7
		If IsRunning()
			Stop()
		EndIf
	EndIf
EndEvent

Event OnQuestShutdown()
	bAggroStart = False
	CancelTimer(MusicAggroTimerId)
	CancelTimer(WaveActiveTimerID)
	CancelTimer(WavePauseTimerID)
	CancelTimer(7)
	UnregisterForAllEvents()
	If LightEnableMarker != None && LightEnableMarker.GetReference() != None
		LightEnableMarker.GetReference().Disable(False)
	EndIf
	If JukeboxSoundMarker != None && JukeboxSoundMarker.GetReference() != None
		JukeboxSoundMarker.GetReference().Disable(False)
	EndIf
EndEvent
