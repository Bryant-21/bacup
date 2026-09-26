Function EnableLocalAlias(ReferenceAlias akAlias)
	If akAlias != None && akAlias.GetReference() != None
		akAlias.GetReference().EnableNoWait()
	EndIf
EndFunction

Function DisableLocalAlias(ReferenceAlias akAlias)
	If akAlias != None && akAlias.GetReference() != None
		akAlias.GetReference().DisableNoWait()
	EndIf
EndFunction

Function EnableLocalCollection(RefCollectionAlias akAliases)
	Int index = 0
	While akAliases != None && index < akAliases.GetCount()
		ObjectReference targetRef = akAliases.GetAt(index)
		If targetRef != None
			targetRef.EnableNoWait()
		EndIf
		index += 1
	EndWhile
EndFunction

Function DisableLocalCollection(RefCollectionAlias akAliases)
	Int index = 0
	While akAliases != None && index < akAliases.GetCount()
		ObjectReference targetRef = akAliases.GetAt(index)
		If targetRef != None
			targetRef.DisableNoWait()
		EndIf
		index += 1
	EndWhile
EndFunction

Function SetLocalHumanStage(Int aiStage)
	If !IsStageDone(aiStage)
		SetStage(aiStage)
	EndIf
EndFunction

Function StartLocalHumanWave(String asWaveID)
	DefaultQuestEncounterWaveScript waves = (Self as Quest) as DefaultQuestEncounterWaveScript
	If waves != None
		waves.StartEncounterWaveByID(asWaveID)
	EndIf
EndFunction

Function StopLocalHumanWave(String asWaveID)
	DefaultQuestEncounterWaveScript waves = (Self as Quest) as DefaultQuestEncounterWaveScript
	If waves != None
		waves.StopEncounterWaveByID(asWaveID, False)
	EndIf
EndFunction

Bool Function HasLivingLocalActor(RefCollectionAlias akAliases)
	Int index = 0
	While akAliases != None && index < akAliases.GetCount()
		Actor member = akAliases.GetAt(index) as Actor
		If member != None && !member.IsDead() && !member.IsDisabled()
			Return True
		EndIf
		index += 1
	EndWhile
	Return False
EndFunction

Function Fragment_Stage_0000_Item_00()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If PlayerAlias != None && PlayerAlias.GetReference() != playerRef
			PlayerAlias.ForceRefTo(playerRef)
		EndIf
		If ExpeditionTeam != None && ExpeditionTeam.Find(playerRef) < 0
			ExpeditionTeam.AddRef(playerRef)
		EndIf
	EndIf
EndFunction

Function Fragment_Stage_0011_Item_00()
EndFunction

Function Fragment_Stage_0031_Item_00()
EndFunction

Function Fragment_Stage_0100_Item_00()
EndFunction

Function Fragment_Stage_0150_Item_00()
	SetLocalHumanStage(155)
	EnableLocalCollection(MainAllies)
	EnableLocalAlias(WiseGuyAlly)
	EnableLocalAlias(ShowmanAlly)
	EnableLocalAlias(MuniAlly)
	StartLocalHumanWave("FinaleComCenterExtEnemies")
	StartLocalHumanWave("FinaleComCenterAllies")
EndFunction

Function Fragment_Stage_0155_Item_00()
	DisableLocalAlias(Alias_EnableMarker_Manholes)
EndFunction

Function Fragment_Stage_0200_Item_00()
	SetObjectiveCompleted(500, True)
	If !IsStageDone(800)
		SetObjectiveDisplayed(610, True)
	EndIf
	If !IsStageDone(850)
		SetObjectiveDisplayed(5850, True)
	EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
	EnableLocalCollection(LostDossiers)
EndFunction

Function Fragment_Stage_0400_Item_00()
EndFunction

Function Fragment_Stage_0450_Item_00()
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveDisplayed(500, True)
	SetObjectiveDisplayed(600, True)
	SetObjectiveDisplayed(700, True)
	EnableLocalAlias(Alias_EnableMarker_Manholes)
	SetLocalHumanStage(610)
EndFunction

Function Fragment_Stage_0610_Item_00()
	StartLocalHumanWave("IntroOvergrown")
	StartLocalHumanWave("IntroAuditors")
	SetLocalHumanStage(620)
EndFunction

Function Fragment_Stage_0620_Item_00()
EndFunction

Function Fragment_Stage_0650_Item_00()
	SetObjectiveCompleted(600, True)
	If HasLivingLocalActor(GetAlias(245) as RefCollectionAlias)
		SetLocalHumanStage(750)
	Else
		SetLocalHumanStage(760)
	EndIf
EndFunction

Function Fragment_Stage_0750_Item_00()
	SetObjectiveCompleted(700, True)
EndFunction

Function Fragment_Stage_0760_Item_00()
	SetObjectiveFailed(700, True)
EndFunction

Function Fragment_Stage_0800_Item_00()
	SetObjectiveCompleted(610, True)
EndFunction

Function Fragment_Stage_0850_Item_00()
	SetObjectiveCompleted(5850, True)
	SetObjectiveDisplayed(5900, True)
	If IsStageDone(860)
		SetObjectiveCompleted(5900, True)
	EndIf
	SetObjectiveDisplayed(5910, True)
	If !IsStageDone(851) && !IsStageDone(852) && !IsStageDone(853) && !IsStageDone(854) && !IsStageDone(855)
		SetStage(851 + Utility.RandomInt(0, 4))
	EndIf
EndFunction

Function Fragment_Stage_0851_Item_00()
	EnableLocalAlias(Camera01)
EndFunction

Function Fragment_Stage_0852_Item_00()
	EnableLocalAlias(Camera02)
EndFunction

Function Fragment_Stage_0853_Item_00()
	EnableLocalAlias(Camera03)
EndFunction

Function Fragment_Stage_0854_Item_00()
	EnableLocalAlias(Camera04)
EndFunction

Function Fragment_Stage_0855_Item_00()
	EnableLocalAlias(Camera05)
EndFunction

Function Fragment_Stage_0860_Item_00()
	If IsObjectiveDisplayed(5900)
		SetObjectiveCompleted(5900, True)
	EndIf
EndFunction

Function Fragment_Stage_0870_Item_00()
	If FootageMessage != None
		FootageMessage.Show()
	EndIf
	DisableLocalAlias(Camera01)
	DisableLocalAlias(Camera02)
	DisableLocalAlias(Camera03)
	DisableLocalAlias(Camera04)
	DisableLocalAlias(Camera05)
	SetObjectiveCompleted(5910, True)
EndFunction

Function Fragment_Stage_0899_Item_00()
	SetLocalHumanStage(1000)
EndFunction

Function Fragment_Stage_1000_Item_00()
EndFunction

Function Fragment_Stage_1010_Item_00()
EndFunction

Function Fragment_Stage_1020_Item_00()
EndFunction

Function Fragment_Stage_1025_Item_00()
EndFunction

Function Fragment_Stage_1030_Item_00()
EndFunction

Function Fragment_Stage_1035_Item_00()
EndFunction

Function Fragment_Stage_1040_Item_00()
EndFunction

Function Fragment_Stage_1599_Item_00()
	SetLocalHumanStage(1600)
EndFunction

Function Fragment_Stage_1600_Item_00()
	SetObjectiveDisplayed(1610, True)
EndFunction

Function Fragment_Stage_1700_Item_00()
	SetObjectiveCompleted(1610, True)
	SetLocalHumanStage(2000)
EndFunction

Function Fragment_Stage_1899_Item_00()
	SetLocalHumanStage(2000)
EndFunction

Function Fragment_Stage_2000_Item_00()
EndFunction

Function Fragment_Stage_2599_Item_00()
	SetLocalHumanStage(2600)
EndFunction

Function Fragment_Stage_2600_Item_00()
	SetLocalHumanStage(3000)
EndFunction

Function Fragment_Stage_2899_Item_00()
	SetLocalHumanStage(3000)
EndFunction

Function Fragment_Stage_3000_Item_00()
EndFunction

Function Fragment_Stage_3599_Item_00()
	SetLocalHumanStage(3600)
EndFunction

Function Fragment_Stage_3600_Item_00()
	SetLocalHumanStage(4500)
EndFunction

Function Fragment_Stage_4499_Item_00()
	SetLocalHumanStage(4500)
EndFunction

Function Fragment_Stage_4500_Item_00()
	SetObjectiveDisplayed(4500, True)
EndFunction

Function Fragment_Stage_4510_Item_00()
	StartLocalHumanWave("PollinatorsBossFight01")
EndFunction

Function Fragment_Stage_4511_Item_00()
	StartLocalHumanWave("PollinatorsBossFight02")
EndFunction

Function Fragment_Stage_4512_Item_00()
	StartLocalHumanWave("PollinatorsBossFight01")
EndFunction

Function Fragment_Stage_4513_Item_00()
	StartLocalHumanWave("PollinatorsBossFight02")
EndFunction

Function Fragment_Stage_4600_Item_00()
	SetObjectiveCompleted(4500, True)
	Actor boss = None
	If Sporemaster != None
		boss = Sporemaster.GetActorReference()
	EndIf
	If boss != None
		boss.EnableNoWait()
		If boss.IsDead()
			boss.Resurrect()
		EndIf
	EndIf
	SetObjectiveDisplayed(4510, True)
	StartLocalHumanWave("FinaleCityCenterEnemies")
	SetLocalHumanStage(4510)
EndFunction

Function Fragment_Stage_4699_Item_00()
	SetLocalHumanStage(4700)
EndFunction

Function Fragment_Stage_4700_Item_00()
	SetObjectiveCompleted(4500, True)
	SetObjectiveCompleted(4510, True)
	StopLocalHumanWave("FinaleComCenterExtEnemies")
	StopLocalHumanWave("FinaleCityCenterEnemies")
	StopLocalHumanWave("PollinatorsBossFight01")
	StopLocalHumanWave("PollinatorsBossFight02")
	DisableLocalCollection(DisableCivilians)
	EnableLocalCollection(FinaleCivilians)
	EnableLocalAlias(MayorTimFinale)
	EnableLocalAlias(ButtercupFinale)
	SetObjectiveDisplayed(4710, True)
EndFunction

Function Fragment_Stage_5200_Item_00()
	SetObjectiveCompleted(4710, True)
EndFunction

Function Fragment_Stage_5400_Item_00()
EndFunction

Function Fragment_Stage_5600_Item_00()
EndFunction

Function Fragment_Stage_9000_Item_00()
	If !IsCompleted()
		CompleteQuest()
	EndIf
EndFunction
