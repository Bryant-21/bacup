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

Function UnlockLocalAlias(ReferenceAlias akAlias)
	If akAlias != None && akAlias.GetReference() != None
		akAlias.GetReference().Lock(False)
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

Function SetLocalPitt02Stage(Int aiStage)
	If !IsStageDone(aiStage)
		SetStage(aiStage)
	EndIf
EndFunction

Function Fragment_Stage_0000_Item_00()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If Alias_ExpeditionLeader != None && Alias_ExpeditionLeader.GetReference() != playerRef
			Alias_ExpeditionLeader.ForceRefTo(playerRef)
		EndIf
		If Alias_ExpeditionTeam != None && Alias_ExpeditionTeam.Find(playerRef) < 0
			Alias_ExpeditionTeam.AddRef(playerRef)
		EndIf
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	EnableLocalCollection(Alias_Actors_ExteriorEnemies)
	EnableLocalAlias(Alias_Danilo_Trench)
EndFunction

Function Fragment_Stage_0200_Item_00()
	EnableLocalAlias(Alias_Danilo_Sanctum)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetObjectiveDisplayed(410, True)
EndFunction

Function Fragment_Stage_0450_Item_00()
	SetObjectiveCompleted(410, True)
EndFunction

Function Fragment_Stage_0460_Item_00()
EndFunction

Function Fragment_Stage_0500_Item_00()
EndFunction

Function Fragment_Stage_0600_Item_00()
	SetObjectiveDisplayed(610, True)
	EnableLocalAlias(Alias_Danilo_Trench)
EndFunction

Function Fragment_Stage_0799_Item_00()
	SetLocalPitt02Stage(800)
EndFunction

Function Fragment_Stage_0800_Item_00()
	SetObjectiveCompleted(610, True)
EndFunction

Function Fragment_Stage_0810_Item_00()
	If XPD_Pitt02_Mission_Danilo_ExitLZ != None && !XPD_Pitt02_Mission_Danilo_ExitLZ.IsPlaying()
		XPD_Pitt02_Mission_Danilo_ExitLZ.Start()
	EndIf
EndFunction

Function Fragment_Stage_0820_Item_00()
	DisableLocalAlias(Alias_Danilo_Trench)
	SetLocalPitt02Stage(1000)
EndFunction

Function Fragment_Stage_1000_Item_00()
EndFunction

Function Fragment_Stage_1100_Item_00()
EndFunction

Function Fragment_Stage_1200_Item_00()
EndFunction

Function Fragment_Stage_1300_Item_00()
EndFunction

Function Fragment_Stage_1400_Item_00()
	If XPD_Pitt02_Mission_Laborer_GatherAndDeposit_Complete != None && !XPD_Pitt02_Mission_Laborer_GatherAndDeposit_Complete.IsPlaying()
		XPD_Pitt02_Mission_Laborer_GatherAndDeposit_Complete.Start()
	EndIf
EndFunction

Function Fragment_Stage_1599_Item_00()
	SetLocalPitt02Stage(1600)
EndFunction

Function Fragment_Stage_1600_Item_00()
	SetObjectiveDisplayed(1610, True)
EndFunction

Function Fragment_Stage_1700_Item_00()
	SetObjectiveCompleted(1610, True)
	SetObjectiveDisplayed(6000, True)
EndFunction

Function Fragment_Stage_1710_Item_00()
	If XPD_Pitt02_Mission_Danilo_ExitOutpost != None && !XPD_Pitt02_Mission_Danilo_ExitOutpost.IsPlaying()
		XPD_Pitt02_Mission_Danilo_ExitOutpost.Start()
	EndIf
EndFunction

Function Fragment_Stage_1720_Item_00()
	DisableLocalAlias(Alias_Danilo_Trench)
EndFunction

Function Fragment_Stage_1750_Item_00()
	SetObjectiveCompleted(6000, True)
	SetObjectiveDisplayed(6010, True)
	SetObjectiveDisplayed(6020, True)
	SetObjectiveDisplayed(6025, True)
EndFunction

Function Fragment_Stage_1760_Item_00()
	SetObjectiveDisplayed(6010, True)
	SetObjectiveDisplayed(6020, True)
	SetObjectiveDisplayed(6025, True)
EndFunction

Function Fragment_Stage_1800_Item_00()
	SetObjectiveDisplayed(6010, True)
EndFunction

Function Fragment_Stage_1810_Item_00()
	SetObjectiveCompleted(6010, True)
	SetObjectiveDisplayed(6015, True)
EndFunction

Function Fragment_Stage_1820_Item_00()
	SetObjectiveCompleted(6015, True)
EndFunction

Function Fragment_Stage_1850_Item_00()
	SetObjectiveDisplayed(1930, True)
	SetLocalPitt02Stage(2000)
EndFunction

Function Fragment_Stage_1899_Item_00()
	SetLocalPitt02Stage(2000)
EndFunction

Function Fragment_Stage_2000_Item_00()
EndFunction

Function Fragment_Stage_2200_Item_00()
EndFunction

Function Fragment_Stage_2300_Item_00()
EndFunction

Function Fragment_Stage_2310_Item_00()
EndFunction

Function Fragment_Stage_2320_Item_00()
EndFunction

Function Fragment_Stage_2330_Item_00()
EndFunction

Function Fragment_Stage_2400_Item_00()
EndFunction

Function Fragment_Stage_2599_Item_00()
	SetLocalPitt02Stage(2600)
EndFunction

Function Fragment_Stage_2600_Item_00()
	SetObjectiveDisplayed(2500, True)
EndFunction

Function Fragment_Stage_2700_Item_00()
	SetObjectiveCompleted(2500, True)
	SetLocalPitt02Stage(3000)
EndFunction

Function Fragment_Stage_2899_Item_00()
	SetLocalPitt02Stage(3000)
EndFunction

Function Fragment_Stage_3000_Item_00()
EndFunction

Function Fragment_Stage_3010_Item_00()
EndFunction

Function Fragment_Stage_3020_Item_00()
EndFunction

Function Fragment_Stage_3030_Item_00()
EndFunction

Function Fragment_Stage_3100_Item_00()
	SetObjectiveCompleted(1930, True)
	SetObjectiveDisplayed(4000, True)
	If XPD_Pitt02_Mission_CathedralPAIntro != None && !XPD_Pitt02_Mission_CathedralPAIntro.IsPlaying()
		XPD_Pitt02_Mission_CathedralPAIntro.Start()
	EndIf
EndFunction

Function Fragment_Stage_3110_Item_00()
	EnableLocalAlias(Alias_SoundMarker_SanctumOrganBattle)
	DisableLocalAlias(Alias_SoundMarker_SanctumOrgan)
EndFunction

Function Fragment_Stage_3120_Item_00()
	DisableLocalAlias(Alias_SoundMarker_SanctumOrganBattle)
EndFunction

Function Fragment_Stage_3200_Item_00()
	EnableLocalAlias(Alias_Danilo_Sanctum_DefendNPC)
EndFunction

Function Fragment_Stage_3300_Item_00()
	If XPD_Pitt02_Mission_Danilo_DefendNPC_Hello != None && !XPD_Pitt02_Mission_Danilo_DefendNPC_Hello.IsPlaying()
		XPD_Pitt02_Mission_Danilo_DefendNPC_Hello.Start()
	EndIf
EndFunction

Function Fragment_Stage_3310_Item_00()
EndFunction

Function Fragment_Stage_3320_Item_00()
EndFunction

Function Fragment_Stage_3330_Item_00()
EndFunction

Function Fragment_Stage_3340_Item_00()
EndFunction

Function Fragment_Stage_3400_Item_00()
	If XPD_Pitt02_Mission_Danilo_DefendNPC_ModuleCompleted != None && !XPD_Pitt02_Mission_Danilo_DefendNPC_ModuleCompleted.IsPlaying()
		XPD_Pitt02_Mission_Danilo_DefendNPC_ModuleCompleted.Start()
	EndIf
EndFunction

Function Fragment_Stage_3450_Item_00()
	DisableLocalAlias(Alias_Danilo_Sanctum_DefendNPC)
EndFunction

Function Fragment_Stage_3599_Item_00()
	SetLocalPitt02Stage(3600)
EndFunction

Function Fragment_Stage_3600_Item_00()
	SetObjectiveCompleted(4000, True)
	SetLocalPitt02Stage(4500)
EndFunction

Function Fragment_Stage_4499_Item_00()
	SetLocalPitt02Stage(4500)
EndFunction

Function Fragment_Stage_4500_Item_00()
	SetObjectiveDisplayed(4500, True)
	UnlockLocalAlias(Alias_Door_BossRoom)
EndFunction

Function Fragment_Stage_4600_Item_00()
	SetObjectiveCompleted(4500, True)
	SetObjectiveDisplayed(4510, True)
	EnableLocalAlias(Alias_MissionBoss_Foreman)
EndFunction

Function Fragment_Stage_4650_Item_00()
	EnableLocalCollection(Alias_MissionBossAdds_Caged)
	EnableLocalCollection(Alias_MissionBossAdds_Initial)
	If XPD_Pitt02_Mission_Foreman_HalfHealth != None && !XPD_Pitt02_Mission_Foreman_HalfHealth.IsPlaying()
		XPD_Pitt02_Mission_Foreman_HalfHealth.Start()
	EndIf
EndFunction

Function Fragment_Stage_4660_Item_00()
	EnableLocalAlias(Alias_MissionBoss_Foreman)
EndFunction

Function Fragment_Stage_4699_Item_00()
	SetLocalPitt02Stage(4700)
EndFunction

Function Fragment_Stage_4700_Item_00()
	SetObjectiveCompleted(4510, True)
	SetObjectiveDisplayed(4600, True)
EndFunction

Function Fragment_Stage_4750_Item_00()
	SetObjectiveCompleted(4650, True)
	UnlockLocalAlias(Alias_Door_ExitGate)
EndFunction

Function Fragment_Stage_5000_Item_00()
	SetObjectiveCompleted(4600, True)
	EnableLocalAlias(Alias_Danilo_Trench)
EndFunction

Function Fragment_Stage_5001_Item_00()
	EnableLocalCollection(Alias_Actors_HelplessSurvivors)
EndFunction

Function Fragment_Stage_5005_Item_00()
	SetObjectiveDisplayed(6030, True)
	SetObjectiveDisplayed(6031, True)
	SetObjectiveDisplayed(6032, True)
	SetObjectiveDisplayed(6033, True)
EndFunction

Function Fragment_Stage_5010_Item_00()
	SetObjectiveCompleted(6031, True)
	If XPD_Pitt02_Mission_HelplessSurvivor_01_TravelToSafety != None && !XPD_Pitt02_Mission_HelplessSurvivor_01_TravelToSafety.IsPlaying()
		XPD_Pitt02_Mission_HelplessSurvivor_01_TravelToSafety.Start()
	EndIf
EndFunction

Function Fragment_Stage_5015_Item_00()
	SetObjectiveFailed(6031, True)
EndFunction

Function Fragment_Stage_5018_Item_00()
EndFunction

Function Fragment_Stage_5019_Item_00()
	DisableLocalAlias(Alias_Actor_HelplessSurvivor_01)
EndFunction

Function Fragment_Stage_5020_Item_00()
	SetObjectiveCompleted(6032, True)
	If XPD_Pitt02_Mission_HelplessSurvivor_02_TravelToSafety != None && !XPD_Pitt02_Mission_HelplessSurvivor_02_TravelToSafety.IsPlaying()
		XPD_Pitt02_Mission_HelplessSurvivor_02_TravelToSafety.Start()
	EndIf
EndFunction

Function Fragment_Stage_5025_Item_00()
	SetObjectiveFailed(6032, True)
EndFunction

Function Fragment_Stage_5028_Item_00()
EndFunction

Function Fragment_Stage_5029_Item_00()
	DisableLocalAlias(Alias_Actor_HelplessSurvivor_02)
EndFunction

Function Fragment_Stage_5030_Item_00()
	SetObjectiveCompleted(6033, True)
	If XPD_Pitt02_Mission_HelplessSurvivor_03_TravelToSafety != None && !XPD_Pitt02_Mission_HelplessSurvivor_03_TravelToSafety.IsPlaying()
		XPD_Pitt02_Mission_HelplessSurvivor_03_TravelToSafety.Start()
	EndIf
EndFunction

Function Fragment_Stage_5035_Item_00()
	SetObjectiveFailed(6033, True)
EndFunction

Function Fragment_Stage_5038_Item_00()
EndFunction

Function Fragment_Stage_5039_Item_00()
	DisableLocalAlias(Alias_Actor_HelplessSurvivor_03)
EndFunction

Function Fragment_Stage_5090_Item_00()
EndFunction

Function Fragment_Stage_5100_Item_00()
	SetObjectiveCompleted(6030, True)
EndFunction

Function Fragment_Stage_5110_Item_00()
	SetObjectiveFailed(6030, True)
EndFunction

Function Fragment_Stage_5200_Item_00()
	SetObjectiveDisplayed(5210, True)
	SetObjectiveDisplayed(4710, True)
EndFunction

Function Fragment_Stage_5210_Item_00()
	SetObjectiveCompleted(4710, True)
EndFunction

Function Fragment_Stage_5400_Item_00()
	SetObjectiveCompleted(5210, True)
EndFunction

Function Fragment_Stage_5600_Item_00()
EndFunction

Function Fragment_Stage_6900_Item_00()
	SetObjectiveDisplayed(6020, True)
	SetObjectiveDisplayed(6025, True)
EndFunction

Function Fragment_Stage_7000_Item_00()
	SetObjectiveCompleted(6020, True)
EndFunction

Function Fragment_Stage_7010_Item_00()
	SetObjectiveFailed(6020, True)
EndFunction

Function Fragment_Stage_7100_Item_00()
	SetObjectiveCompleted(6020, True)
EndFunction

Function Fragment_Stage_7110_Item_00()
	SetObjectiveFailed(6020, True)
EndFunction

Function Fragment_Stage_7200_Item_00()
	SetObjectiveCompleted(6025, True)
EndFunction

Function Fragment_Stage_7210_Item_00()
	SetObjectiveFailed(6025, True)
EndFunction

Function Fragment_Stage_7300_Item_00()
	SetObjectiveCompleted(6020, True)
	SetObjectiveCompleted(6025, True)
EndFunction

Function Fragment_Stage_7400_Item_00()
	If IsObjectiveDisplayed(6020) && !IsObjectiveCompleted(6020)
		SetObjectiveFailed(6020, True)
	EndIf
	If IsObjectiveDisplayed(6025) && !IsObjectiveCompleted(6025)
		SetObjectiveFailed(6025, True)
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	If !IsCompleted()
		CompleteQuest()
	EndIf
EndFunction
