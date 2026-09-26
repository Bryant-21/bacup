Int Function FinalWaveTimerID() Global
	Return 64311
EndFunction

Event OnQuestInit()
	CancelTimer(FinalWaveTimerID())
	meatbags = New ObjectReference[0]
	usedSpawnIndexs = New Int[0]
	pendingEWSTimers = New Int[0]
EndEvent

Event OnQuestShutdown()
	CleanupEventWorld()
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID != FinalWaveTimerID()
		Return
	EndIf
	If !IsRunning() || IsStageDone(650) || IsEventOver()
		Return
	EndIf
	; The counterattack waves hand over to the final waves instead of stacking on them.
	StopWaves(3, 5)
	Int index = 0
	While pendingEWSTimers != None && index < pendingEWSTimers.Length
		StartWaves(pendingEWSTimers[index], pendingEWSTimers[index])
		index += 1
	EndWhile
	pendingEWSTimers = New Int[0]
EndEvent

Bool Function IsEventOver()
	Return IsStageDone(9000) || IsStageDone(9998) || IsStageDone(9999)
EndFunction

DefaultQuestEncounterWaveScript Function WaveScript()
	Quest owner = Self as Quest
	Return owner as DefaultQuestEncounterWaveScript
EndFunction

DefaultCounterQuest Function MeatbagCounter()
	Quest owner = Self as Quest
	Return owner as DefaultCounterQuest
EndFunction

Function StartWaves(Int aiFirst, Int aiLast)
	DefaultQuestEncounterWaveScript waves = WaveScript()
	If waves == None
		Return
	EndIf
	; EncounterWaves repeats IDStrings (three SuperMutantMelee rows), so waves are addressed by bound row index.
	Int index = aiFirst
	While index <= aiLast
		waves.StartEncounterWave(index)
		index += 1
	EndWhile
EndFunction

Function StopWaves(Int aiFirst, Int aiLast)
	DefaultQuestEncounterWaveScript waves = WaveScript()
	If waves == None
		Return
	EndIf
	Int index = aiFirst
	While index <= aiLast
		waves.StopEncounterWave(index, False)
		index += 1
	EndWhile
EndFunction

Function StartMeatbagCombat()
	If IsEventOver() || IsStageDone(600)
		Return
	EndIf
	StartWaves(0, 2)
EndFunction

Function StartCounterattack()
	If IsEventOver() || IsStageDone(650)
		Return
	EndIf
	StopWaves(0, 2)
	StartWaves(3, 5)
	pendingEWSTimers = New Int[3]
	pendingEWSTimers[0] = 6
	pendingEWSTimers[1] = 7
	pendingEWSTimers[2] = 8

	; The final waves take over halfway between the counterattack and the spawn stop, which the record
	; places 60 s before the quest timer (540 s objective 70 against the 600 s quest timer).
	Float delay = 120.0
	Quest owner = Self as Quest
	B21:QuestTimer questTimer = owner as B21:QuestTimer
	If questTimer != None && questTimer.IsQuestTimerRunning()
		delay = (questTimer.GetQuestTimerRemaining() - 60.0) / 2.0
	EndIf
	If delay < 30.0
		delay = 30.0
	EndIf
	CancelTimer(FinalWaveTimerID())
	StartTimer(delay, FinalWaveTimerID())
EndFunction

Function StopEventWaves()
	CancelTimer(FinalWaveTimerID())
	pendingEWSTimers = New Int[0]
	DefaultQuestEncounterWaveScript waves = WaveScript()
	If waves != None
		waves.StopAllEncounterWaves(False)
	EndIf
EndFunction

Function SpawnMeatbags()
	If MeatbagFillAlias == None || MeatbagSpawnPoints == None || Form_Meatbag == None
		Return
	EndIf
	; The quest allows repeated stages, so a second stage 200 must not hang a second set.
	If MeatbagFillAlias.GetCount() > 0
		Return
	EndIf

	Int pointCount = MeatbagSpawnPoints.GetCount()
	DefaultCounterQuest counter = MeatbagCounter()
	Int wanted = pointCount
	If counter != None && counter.TargetValue > 0 && counter.TargetValue < pointCount
		wanted = counter.TargetValue
	EndIf
	If wanted <= 0
		Return
	EndIf

	Int[] candidates = New Int[pointCount]
	Int index = 0
	While index < pointCount
		candidates[index] = index
		index += 1
	EndWhile

	usedSpawnIndexs = New Int[0]
	meatbags = New ObjectReference[0]
	While usedSpawnIndexs.Length < wanted && candidates.Length > 0
		Int pick = Utility.RandomInt(0, candidates.Length - 1)
		Int pointIndex = candidates[pick]
		candidates.Remove(pick)
		ObjectReference spawnPoint = MeatbagSpawnPoints.GetAt(pointIndex)
		If spawnPoint != None
			ObjectReference meatbag = spawnPoint.PlaceAtMe(Form_Meatbag, 1, True, False, False)
			If meatbag != None
				usedSpawnIndexs.Add(pointIndex)
				meatbags.Add(meatbag)
				MeatbagFillAlias.AddRef(meatbag)
			EndIf
		EndIf
	EndWhile

	; The objective reads "/6"; if a post failed to resolve, the goal shrinks to what actually hangs.
	If counter != None && meatbags.Length > 0 && meatbags.Length < counter.TargetValue
		counter.TargetValue = meatbags.Length
	EndIf
	PublishMeatbagCount()
EndFunction

Int Function MeatbagGoal()
	DefaultCounterQuest counter = MeatbagCounter()
	If counter != None && counter.TargetValue > 0
		Return counter.TargetValue
	EndIf
	If meatbags != None
		Return meatbags.Length
	EndIf
	Return 0
EndFunction

Function PublishMeatbagCount()
	Int destroyed = MeatbagGoal()
	If MeatbagFillAlias != None
		destroyed -= MeatbagFillAlias.GetCount()
	EndIf
	If destroyed < 0
		destroyed = 0
	EndIf
	Quest owner = Self as Quest
	B21:QuestVariables questVariables = owner as B21:QuestVariables
	If questVariables != None
		questVariables.SetVariable("MeatBagsDestroyed", destroyed as Float)
	EndIf
EndFunction

Function MeatbagDestroyed(ObjectReference akMeatbag)
	If akMeatbag == None || MeatbagFillAlias == None || MeatbagFillAlias.Find(akMeatbag) < 0
		Return
	EndIf
	MeatbagFillAlias.RemoveRef(akMeatbag)
	PublishMeatbagCount()
	If MeatbagFillAlias.GetCount() > 0 || !IsRunning() || IsEventOver()
		Return
	EndIf
	; DefaultCounterQuest is FO4's stock counter: its private count survives a stopped run, so the
	; empty collection decides completion and the counter only supplies the bound goal and stage.
	Int allGoneStage = 500
	DefaultCounterQuest counter = MeatbagCounter()
	If counter != None && counter.MyStage >= 0
		allGoneStage = counter.MyStage
	EndIf
	If !IsStageDone(allGoneStage)
		SetStage(allGoneStage)
	EndIf
EndFunction

Function SetRadiationLevel(Int aiLevel)
	; RadStages rows follow their fill aliases: 0 RadiationHazard_Default (crater before the scrubber runs),
	; 1 RadiationHazard_Token (scrubber running), 2 RadiationHazard_Deadly and 3 _DeadlyNearScrubber (scrubber down).
	If RadStages == None
		Return
	EndIf
	Int index = 0
	While index < RadStages.Length
		Radiation_Stage row = RadStages[index]
		If row != None && row.FillAlias != None
			Bool wanted = (aiLevel == 0 && index == 0) || (aiLevel == 1 && index == 1) || (aiLevel >= 2 && index >= 2)
			ObjectReference hazardRef = row.FillAlias.GetReference()
			If hazardRef == None && wanted
				hazardRef = PlaceRadiationHazard(row)
			EndIf
			If hazardRef != None
				If wanted
					hazardRef.Enable(False)
				Else
					hazardRef.Disable(False)
				EndIf
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

ObjectReference Function PlaceRadiationHazard(Radiation_Stage akRow)
	If akRow.HazardForm == None || akRow.SpawnPoint == None
		Return None
	EndIf
	ObjectReference spawnPoint = akRow.SpawnPoint.GetReference()
	If spawnPoint == None
		Return None
	EndIf
	ObjectReference hazardRef = spawnPoint.PlaceAtMe(akRow.HazardForm, 1, True, True, False)
	If hazardRef == None
		Return None
	EndIf
	hazardRef.MoveTo(spawnPoint, akRow.xOffset, akRow.yOffset, 0.0, True)
	akRow.FillAlias.ForceRefTo(hazardRef)
	Return hazardRef
EndFunction

Function CleanupEventWorld()
	CancelTimer(FinalWaveTimerID())
	pendingEWSTimers = New Int[0]

	Int index = 0
	While meatbags != None && index < meatbags.Length
		ObjectReference meatbag = meatbags[index]
		If meatbag != None
			meatbag.Disable(False)
			meatbag.Delete()
		EndIf
		index += 1
	EndWhile
	meatbags = New ObjectReference[0]
	usedSpawnIndexs = New Int[0]
	If MeatbagFillAlias != None
		MeatbagFillAlias.RemoveAll()
	EndIf

	index = 0
	While RadStages != None && index < RadStages.Length
		Radiation_Stage row = RadStages[index]
		If row != None && row.FillAlias != None
			ObjectReference hazardRef = row.FillAlias.GetReference()
			If hazardRef != None
				hazardRef.Disable(False)
				hazardRef.Delete()
			EndIf
			row.FillAlias.Clear()
		EndIf
		index += 1
	EndWhile
EndFunction
