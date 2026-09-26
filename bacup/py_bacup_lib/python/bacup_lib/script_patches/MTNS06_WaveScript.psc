Event OnTimer(Int aiTimerID)
	If aiTimerID == 8601
		AssignNewWaveActors()
		If IsRunning() && !IsStageDone(300) && !IsStageDone(325)
			StartTimer(2.0, 8601)
		EndIf
	Else
		parent.OnTimer(aiTimerID)
	EndIf
EndEvent

Event OnQuestShutdown()
	CancelTimer(8601)
	parent.OnQuestShutdown()
EndEvent

Function StartCreatureAssignment()
	Linker = 0
	CancelTimer(8601)
	StartTimer(2.0, 8601)
EndFunction

Function StopCreatureAssignment()
	CancelTimer(8601)
EndFunction

Function AssignNewWaveActors()
	Int waveIndex = 0
	While EncounterWaves && waveIndex < EncounterWaves.Length
		RefCollectionAlias waveCollection = EncounterWaves[waveIndex].WaveRefCollection
		Int index = 0
		While waveCollection != None && index < waveCollection.GetCount()
			AssignWaveActor(waveCollection.GetActorAt(index))
			index += 1
		EndWhile
		waveIndex += 1
	EndWhile
EndFunction

Function AssignWaveActor(Actor akActor)
	If akActor == None || akActor.IsDead() || akActor.HasKeyword(MTNS06_Uranium_CreatureBoss_Keyword)
		Return
	EndIf
	If (MeleeCreatures != None && MeleeCreatures.Find(akActor) >= 0) || (RangedCreatures != None && RangedCreatures.Find(akActor) >= 0)
		Return
	EndIf

	; The travel packages on AllCreatures walk each creature to the extractor linked through LinkCustom01-03.
	Linker = (Linker % 3) + 1
	If Linker == 1 && ExtractorAlias01 != None && ExtractorAlias01.GetReference() != None
		akActor.SetLinkedRef(ExtractorAlias01.GetReference(), LinkCustom01)
	ElseIf Linker == 2 && ExtractorAlias02 != None && ExtractorAlias02.GetReference() != None
		akActor.SetLinkedRef(ExtractorAlias02.GetReference(), LinkCustom02)
	ElseIf Linker == 3 && ExtractorAlias03 != None && ExtractorAlias03.GetReference() != None
		akActor.SetLinkedRef(ExtractorAlias03.GetReference(), LinkCustom03)
	EndIf

	RefCollectionAlias roleCollection = MeleeCreatures
	If LvlMoleMinerRangedMTNS06 != None && akActor.GetActorBase() == LvlMoleMinerRangedMTNS06
		roleCollection = RangedCreatures
	EndIf
	If roleCollection != None
		roleCollection.AddRef(akActor)
		Quests:_Default:SetPreferredCombatTargets preferred = roleCollection as Quests:_Default:SetPreferredCombatTargets
		If preferred != None
			preferred.ApplyPreferredCombatTarget(akActor)
		EndIf
	EndIf
	akActor.EvaluatePackage()
EndFunction
