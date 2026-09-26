Function SetReferenceEnabled(ObjectReference akReference, Bool abEnabled)
	If akReference == None
		Return
	EndIf
	If abEnabled
		akReference.Enable(False)
	Else
		akReference.Disable(False)
	EndIf
EndFunction

Bool Function PlayerParticipating()
	Quest owner = Self as Quest
	DefaultEventQuest eventQuest = owner as DefaultEventQuest
	Return eventQuest == None || eventQuest.IsPlayerParticipating()
EndFunction

Function CastOnParticipant(Spell akSpell)
	Actor playerRef = Game.GetPlayer()
	If akSpell != None && playerRef != None && PlayerParticipating()
		akSpell.Cast(playerRef, playerRef)
	EndIf
EndFunction

E09A_DestructibleCrystalsScript Function CrystalsCollection()
	If CrystalsAliasArrays == None || CrystalsAliasArrays.Length == 0
		Return None
	EndIf
	Return CrystalsAliasArrays[0] as E09A_DestructibleCrystalsScript
EndFunction

E09A_CrystalHengeScript Function HengeCollection(Int aiPhase)
	; CrystalsAliasArrays holds the coaster crystals first, then one henge collection per boss phase.
	Int index = aiPhase + 1
	If CrystalsAliasArrays == None || aiPhase < 0 || index >= CrystalsAliasArrays.Length
		Return None
	EndIf
	Return CrystalsAliasArrays[index] as E09A_CrystalHengeScript
EndFunction

CreatureUltraciteAbominationScript Function BossScript()
	If Alias_Boss == None
		Return None
	EndIf
	Return Alias_Boss as CreatureUltraciteAbominationScript
EndFunction

Function SetDecorationsEnabled(Bool abEnabled)
	SetReferenceEnabled(FundamentalsMarkerRef, abEnabled)
	Int index = 0
	While DecorationEnableParents != None && index < DecorationEnableParents.Length
		; Entry 0 dresses the coaster for the whole event; entries 1-3 raise each later phase's henge.
		If !abEnabled || index == 0
			SetReferenceEnabled(DecorationEnableParents[index], abEnabled)
		EndIf
		index += 1
	EndWhile
EndFunction

Function RemoveFissureHazards()
	Int index = 0
	While RadHazards != None && index < RadHazards.Length
		If RadHazards[index] != None
			RadHazards[index].Disable(False)
			RadHazards[index].Delete()
		EndIf
		index += 1
	EndWhile
	RadHazards = None
EndFunction

Function PlaceFissureHazard(Int aiPhase)
	If RadiationHazardForm == None || FissureAliasArray == None || aiPhase < 0 || aiPhase >= FissureAliasArray.Length || FissureAliasArray[aiPhase] == None
		Return
	EndIf
	ObjectReference fissure = FissureAliasArray[aiPhase].GetReference()
	If fissure == None
		Return
	EndIf
	ObjectReference hazardRef = fissure.PlaceAtMe(RadiationHazardForm, 1, False, False, False)
	If hazardRef == None
		Return
	EndIf
	If RadHazards == None
		RadHazards = New ObjectReference[0]
	EndIf
	RadHazards.Add(hazardRef)
EndFunction

Function BeginEvent()
	CancelTimer(9001)
	RemoveFissureHazards()
	Int phase = 0
	While phase < 4
		E09A_CrystalHengeScript henge = HengeCollection(phase)
		If henge != None
			henge.ResetHenge()
		EndIf
		phase += 1
	EndWhile
	SetDecorationsEnabled(True)
	If EventWeather != None
		; Non-override so a nuke zone's forced weather still wins.
		EventWeather.SetActive(False, True)
	EndIf
	E09A_DestructibleCrystalsScript crystals = CrystalsCollection()
	If crystals != None
		crystals.BeginCrystalClearing()
	EndIf
EndFunction

Function BeginBossPhase(Int aiPhase)
	If aiPhase > 0 && DecorationEnableParents != None && aiPhase < DecorationEnableParents.Length
		SetReferenceEnabled(DecorationEnableParents[aiPhase], True)
	EndIf
	PlaceFissureHazard(aiPhase)
	CastOnParticipant(EarthquakeSpell)

	CreatureUltraciteAbominationScript boss = BossScript()
	If aiPhase == 0 && boss != None
		boss.BeginBossFight()
	EndIf
	E09A_CrystalHengeScript henge = HengeCollection(aiPhase)
	If henge != None
		henge.ActivateHenge()
	ElseIf boss != None
		boss.SetBossInvulnerable(False)
	EndIf
EndFunction

Function BeginPhaseTransition()
	CastOnParticipant(LongEarthquakeSpell)
EndFunction

Function EndBossFight()
	CreatureUltraciteAbominationScript boss = BossScript()
	If boss != None
		boss.EndBossFight()
	EndIf
EndFunction

Function FinishEvent(Bool abSucceeded)
	Quest owner = Self as Quest
	DefaultQuestEncounterWaveScript waves = owner as DefaultQuestEncounterWaveScript
	If waves != None
		waves.StopAllEncounterWaves(False)
	EndIf
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && PlayerParticipating()
		If abSucceeded && NWOT_Dialogue_BeatenBoss != None
			playerRef.SetValue(NWOT_Dialogue_BeatenBoss, 1.0)
		ElseIf !abSucceeded && NWOT_Dialogue_FailedBoss != None
			playerRef.SetValue(NWOT_Dialogue_FailedBoss, 1.0)
		EndIf
	EndIf
	; The delay lets stage rewards and the Titan corpse timer resolve before shutdown clears the aliases.
	StartTimer(10.0, 9001)
EndFunction

Function CleanupEventWorld()
	CancelTimer(9001)
	EndBossFight()
	RemoveFissureHazards()
	SetDecorationsEnabled(False)
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID == 9001 && IsRunning() && !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndEvent

Event OnQuestShutdown()
	CleanupEventWorld()
EndEvent
