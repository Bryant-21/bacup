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

Function UnlockLocalCollection(RefCollectionAlias akAliases)
	Int index = 0
	While akAliases != None && index < akAliases.GetCount()
		ObjectReference targetRef = akAliases.GetAt(index)
		If targetRef != None
			targetRef.Lock(False)
		EndIf
		index += 1
	EndWhile
EndFunction

Function SetLocalSensationStage(Int aiStage)
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
	EnableLocalAlias(Alias_EnableMarker_FriendlyNPCs_Boardwalk)
	EnableLocalAlias(Alias_EnableMarker_Manholes)
EndFunction

Function Fragment_Stage_0200_Item_00()
	EnableLocalAlias(Alias_EnableMarker_FriendlyNPCs_Pier)
	UnlockLocalAlias(Alias_Door_BoardwalkToPier)
	UnlockLocalCollection(Alias_Doors_PierHall)
EndFunction

Function Fragment_Stage_0300_Item_00()
	EnableLocalAlias(Alias_EnableMarker_FriendlyNPCs_Aquarium)
	UnlockLocalAlias(Alias_Door_PierToAquarium)
EndFunction

Function Fragment_Stage_0350_Item_00()
	EnableLocalAlias(Alias_EnableMarker_FriendlyNPCs_Boardwalk)
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
	EnableLocalAlias(Alias_ExteriorNPC_01_Veracio)
EndFunction

Function Fragment_Stage_0800_Item_00()
	SetObjectiveCompleted(610, True)
	SetObjectiveDisplayed(615, True)
EndFunction

Function Fragment_Stage_0850_Item_00()
	SetObjectiveCompleted(615, True)
	SetLocalSensationStage(1000)
EndFunction

Function Fragment_Stage_0860_Item_00()
	SetObjectiveDisplayed(6000, True)
	SetObjectiveDisplayed(6010, True)
	SetObjectiveDisplayed(6020, True)
	SetObjectiveDisplayed(6023, True)
	SetObjectiveDisplayed(6025, True)
EndFunction

Function Fragment_Stage_0899_Item_00()
	SetLocalSensationStage(1000)
EndFunction

Function Fragment_Stage_1000_Item_00()
EndFunction

Function Fragment_Stage_1100_Item_00()
EndFunction

Function Fragment_Stage_1200_Item_00()
EndFunction

Function Fragment_Stage_1210_Item_00()
EndFunction

Function Fragment_Stage_1300_Item_00()
EndFunction

Function Fragment_Stage_1599_Item_00()
	SetLocalSensationStage(1600)
EndFunction

Function Fragment_Stage_1600_Item_00()
	SetObjectiveDisplayed(1610, True)
EndFunction

Function Fragment_Stage_1750_Item_00()
	SetObjectiveDisplayed(6010, True)
	SetObjectiveDisplayed(6020, True)
	SetObjectiveDisplayed(6023, True)
	SetObjectiveDisplayed(6025, True)
EndFunction

Function Fragment_Stage_1800_Item_00()
	SetObjectiveCompleted(6010, True)
EndFunction

Function Fragment_Stage_1801_Item_00()
	SetObjectiveFailed(6010, True)
EndFunction

Function Fragment_Stage_1805_Item_00()
	If IsObjectiveDisplayed(6020) && !IsObjectiveCompleted(6020)
		SetObjectiveCompleted(6020, True)
	ElseIf IsObjectiveDisplayed(6023) && !IsObjectiveCompleted(6023)
		SetObjectiveCompleted(6023, True)
	ElseIf IsObjectiveDisplayed(6025) && !IsObjectiveCompleted(6025)
		SetObjectiveCompleted(6025, True)
	EndIf
EndFunction

Function Fragment_Stage_1806_Item_00()
	If IsObjectiveDisplayed(6020) && !IsObjectiveCompleted(6020)
		SetObjectiveFailed(6020, True)
	EndIf
	If IsObjectiveDisplayed(6023) && !IsObjectiveCompleted(6023)
		SetObjectiveFailed(6023, True)
	EndIf
	If IsObjectiveDisplayed(6025) && !IsObjectiveCompleted(6025)
		SetObjectiveFailed(6025, True)
	EndIf
EndFunction

Function Fragment_Stage_1810_Item_00()
	SetObjectiveCompleted(6000, True)
EndFunction

Function Fragment_Stage_1820_Item_00()
	SetObjectiveFailed(6000, True)
EndFunction

Function Fragment_Stage_1850_Item_00()
	SetObjectiveCompleted(1610, True)
	SetObjectiveDisplayed(1700, True)
	UnlockLocalAlias(Alias_Door_BoardwalkToPier)
	UnlockLocalCollection(Alias_Doors_PierHall)
EndFunction

Function Fragment_Stage_1860_Item_00()
	SetLocalSensationStage(2000)
EndFunction

Function Fragment_Stage_1870_Item_00()
	EnableLocalAlias(Alias_Boss_02_Jullian_Pier)
EndFunction

Function Fragment_Stage_1875_Item_00()
	EnableLocalAlias(Alias_Boss_03_Juchi_Pier)
EndFunction

Function Fragment_Stage_1880_Item_00()
EndFunction

Function Fragment_Stage_1885_Item_00()
EndFunction

Function Fragment_Stage_1899_Item_00()
	SetLocalSensationStage(2000)
EndFunction

Function Fragment_Stage_2000_Item_00()
EndFunction

Function Fragment_Stage_2100_Item_00()
EndFunction

Function Fragment_Stage_2200_Item_00()
EndFunction

Function Fragment_Stage_2300_Item_00()
EndFunction

Function Fragment_Stage_2599_Item_00()
	SetLocalSensationStage(2600)
EndFunction

Function Fragment_Stage_2600_Item_00()
	SetObjectiveCompleted(1700, True)
	SetObjectiveDisplayed(2600, True)
	UnlockLocalAlias(Alias_Door_PierToAquarium)
EndFunction

Function Fragment_Stage_2700_Item_00()
	SetObjectiveCompleted(2600, True)
	EnableLocalAlias(Alias_EnableMarker_FriendlyNPCs_Aquarium)
EndFunction

Function Fragment_Stage_2800_Item_00()
	SetLocalSensationStage(3000)
EndFunction

Function Fragment_Stage_2899_Item_00()
	SetLocalSensationStage(3000)
EndFunction

Function Fragment_Stage_3000_Item_00()
EndFunction

Function Fragment_Stage_3100_Item_00()
	EnableLocalAlias(Alias_EnableMarker_EnemyCombatants_Aquarium)
EndFunction

Function Fragment_Stage_3110_Item_00()
EndFunction

Function Fragment_Stage_3200_Item_00()
	EnableLocalAlias(Alias_Actor_DefendNPC_Civilian)
EndFunction

Function Fragment_Stage_3210_Item_00()
EndFunction

Function Fragment_Stage_3220_Item_00()
EndFunction

Function Fragment_Stage_3230_Item_00()
EndFunction

Function Fragment_Stage_3240_Item_00()
EndFunction

Function Fragment_Stage_3290_Item_00()
EndFunction

Function Fragment_Stage_3400_Item_00()
	EnableLocalAlias(Alias_Actor_CarryAndThrow_NaughtyShowman_00)
	EnableLocalAlias(Alias_Actor_CarryAndThrow_NaughtyShowman_01)
	EnableLocalAlias(Alias_Actor_CarryAndThrow_NaughtyShowman_02)
	EnableLocalAlias(Alias_Actor_CarryAndThrow_NaughtyShowman_03)
EndFunction

Function Fragment_Stage_3410_Item_00()
	If XPD_AC02_Mission_NaughtyShowman00_TargetCompleted != None
		XPD_AC02_Mission_NaughtyShowman00_TargetCompleted.Start()
	EndIf
EndFunction

Function Fragment_Stage_3420_Item_00()
EndFunction

Function Fragment_Stage_3430_Item_00()
EndFunction

Function Fragment_Stage_3440_Item_00()
EndFunction

Function Fragment_Stage_3599_Item_00()
	SetLocalSensationStage(3600)
EndFunction

Function Fragment_Stage_3600_Item_00()
	SetLocalSensationStage(4500)
EndFunction

Function Fragment_Stage_4499_Item_00()
	SetLocalSensationStage(4500)
EndFunction

Function Fragment_Stage_4500_Item_00()
	SetObjectiveDisplayed(4510, True)
	UnlockLocalAlias(Alias_Door_Finale)
	EnableLocalAlias(Alias_Marker_WinnersStage)
EndFunction

Function Fragment_Stage_4510_Item_00()
	SetObjectiveCompleted(4510, True)
	SetObjectiveDisplayed(4520, True)
	EnableLocalAlias(Alias_Boss_02_Jullian_Aquarium)
	EnableLocalAlias(Alias_Boss_03_Juchi_Aquarium)
EndFunction

Function Fragment_Stage_4530_Item_00()
EndFunction

Function Fragment_Stage_4540_Item_00()
EndFunction

Function Fragment_Stage_4699_Item_00()
	SetLocalSensationStage(4700)
EndFunction

Function Fragment_Stage_4700_Item_00()
	SetObjectiveCompleted(4520, True)
	SetObjectiveDisplayed(4710, True)
	If XPD_AC02_Mission_Charlotte_AnnounceWinner != None && !XPD_AC02_Mission_Charlotte_AnnounceWinner.IsPlaying()
		XPD_AC02_Mission_Charlotte_AnnounceWinner.Start()
	EndIf
EndFunction

Function Fragment_Stage_5200_Item_00()
	SetObjectiveCompleted(4710, True)
	SetObjectiveDisplayed(5210, True)
EndFunction

Function Fragment_Stage_5400_Item_00()
	SetObjectiveCompleted(5210, True)
EndFunction

Function Fragment_Stage_5600_Item_00()
EndFunction

Function Fragment_Stage_9000_Item_00()
	If !IsCompleted()
		CompleteQuest()
	EndIf
EndFunction
