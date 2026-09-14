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

Function SetLocalPitt01Stage(Int aiStage)
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
	EnableLocalAlias(Alias_Hex_Intro)
	EnableLocalCollection(Alias_ShantyTown_NPCs)
EndFunction

Function Fragment_Stage_0200_Item_00()
	DisableLocalCollection(Alias_PlacedActors_AllExterior_Enemies)
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
	SetObjectiveDisplayed(710, True)
	EnableLocalAlias(Alias_Hex_Intro)
EndFunction

Function Fragment_Stage_0700_Item_00()
	SetObjectiveCompleted(710, True)
	SetObjectiveDisplayed(720, True)
	EnableLocalAlias(Alias_Hex)
EndFunction

Function Fragment_Stage_0760_Item_00()
	DisableLocalAlias(Alias_Hex_Intro)
EndFunction

Function Fragment_Stage_0799_Item_00()
	SetLocalPitt01Stage(800)
EndFunction

Function Fragment_Stage_0800_Item_00()
	SetObjectiveCompleted(710, True)
	SetObjectiveCompleted(720, True)
	SetLocalPitt01Stage(1000)
EndFunction

Function Fragment_Stage_1000_Item_00()
EndFunction

Function Fragment_Stage_1297_Item_00()
	EnableLocalCollection(Alias_ShantyTown_NPCs)
EndFunction

Function Fragment_Stage_1410_Item_00()
	EnableLocalAlias(Alias_ObjMod_DefendNPC_NPC)
	EnableLocalCollection(Alias_ObjMod_DefendNPC_AmmoCrates)
EndFunction

Function Fragment_Stage_1420_Item_00()
EndFunction

Function Fragment_Stage_1430_Item_00()
EndFunction

Function Fragment_Stage_1440_Item_00()
EndFunction

Function Fragment_Stage_1450_Item_00()
EndFunction

Function Fragment_Stage_1500_Item_00()
	SetObjectiveCompleted(1500, True)
EndFunction

Function Fragment_Stage_1525_Item_00()
EndFunction

Function Fragment_Stage_1550_Item_00()
	If SabotageExplosion != None && Alias_Hex_Obj_A03_Props_EnableMarker != None && Alias_Hex_Obj_A03_Props_EnableMarker.GetReference() != None
		Alias_Hex_Obj_A03_Props_EnableMarker.GetReference().PlaceAtMe(SabotageExplosion)
	EndIf
EndFunction

Function Fragment_Stage_1599_Item_00()
	SetLocalPitt01Stage(1600)
EndFunction

Function Fragment_Stage_1600_Item_00()
	SetObjectiveDisplayed(1610, True)
EndFunction

Function Fragment_Stage_1650_Item_00()
	EnableLocalAlias(Alias_ShantyTown_Gate)
	EnableLocalAlias(Alias_Hex_ShantytownGate_XMarker)
	EnableLocalAlias(Alias_Hex)
	UnlockLocalAlias(Alias_ShantyTown_Gate)
EndFunction

Function Fragment_Stage_1660_Item_00()
	SetObjectiveDisplayed(4800, True)
EndFunction

Function Fragment_Stage_1665_Item_00()
	EnableLocalAlias(Alias_Wicker)
EndFunction

Function Fragment_Stage_1670_Item_00()
	SetObjectiveDisplayed(4825, True)
	SetObjectiveDisplayed(4875, True)
EndFunction

Function Fragment_Stage_1675_Item_00()
	SetObjectiveCompleted(4825, True)
	SetObjectiveDisplayed(4850, True)
EndFunction

Function Fragment_Stage_1680_Item_00()
	SetObjectiveCompleted(4850, True)
EndFunction

Function Fragment_Stage_1690_Item_00()
	If Scene_Beggar_Intro != None && !Scene_Beggar_Intro.IsPlaying()
		Scene_Beggar_Intro.Start()
	EndIf
EndFunction

Function Fragment_Stage_1695_Item_00()
	If Scene_Beggar_Reaction != None && !Scene_Beggar_Reaction.IsPlaying()
		Scene_Beggar_Reaction.Start()
	EndIf
EndFunction

Function Fragment_Stage_1699_Item_00()
	SetLocalPitt01Stage(1700)
EndFunction

Function Fragment_Stage_1700_Item_00()
	SetObjectiveCompleted(1610, True)
	SetObjectiveDisplayed(1630, True)
EndFunction

Function Fragment_Stage_1800_Item_00()
	SetObjectiveCompleted(1630, True)
	DisableLocalCollection(Alias_PlacedActors_AllExterior_Enemies)
EndFunction

Function Fragment_Stage_1825_Item_00()
EndFunction

Function Fragment_Stage_1850_Item_00()
	If XPD_Pitt01_Mission_FanaticPA_EnterFoundry_Scene != None && !XPD_Pitt01_Mission_FanaticPA_EnterFoundry_Scene.IsPlaying()
		XPD_Pitt01_Mission_FanaticPA_EnterFoundry_Scene.Start()
	EndIf
EndFunction

Function Fragment_Stage_1925_Item_00()
EndFunction

Function Fragment_Stage_1950_Item_00()
EndFunction

Function Fragment_Stage_1975_Item_00()
EndFunction

Function Fragment_Stage_2000_Item_00()
EndFunction

Function Fragment_Stage_2100_Item_00()
EndFunction

Function Fragment_Stage_2200_Item_00()
EndFunction

Function Fragment_Stage_2400_Item_00()
EndFunction

Function Fragment_Stage_2450_Item_00()
	If XPD_Pitt01_ObjMod_SolveLocks_DocumentsRecovered_Message != None
		XPD_Pitt01_ObjMod_SolveLocks_DocumentsRecovered_Message.Show()
	EndIf
EndFunction

Function Fragment_Stage_2451_Item_00()
	If XPD_Pitt01_ObjMod_SolveLocks_DocumentsRecovered_Message != None
		XPD_Pitt01_ObjMod_SolveLocks_DocumentsRecovered_Message.Show()
	EndIf
EndFunction

Function Fragment_Stage_2452_Item_00()
	If XPD_Pitt01_ObjMod_SolveLocks_DocumentsRecovered_Message != None
		XPD_Pitt01_ObjMod_SolveLocks_DocumentsRecovered_Message.Show()
	EndIf
EndFunction

Function Fragment_Stage_2599_Item_00()
	SetLocalPitt01Stage(2600)
EndFunction

Function Fragment_Stage_2600_Item_00()
	SetLocalPitt01Stage(3000)
EndFunction

Function Fragment_Stage_3000_Item_00()
EndFunction

Function Fragment_Stage_3100_Item_00()
EndFunction

Function Fragment_Stage_3250_Item_00()
EndFunction

Function Fragment_Stage_3251_Item_00()
EndFunction

Function Fragment_Stage_3252_Item_00()
EndFunction

Function Fragment_Stage_3253_Item_00()
EndFunction

Function Fragment_Stage_3254_Item_00()
EndFunction

Function Fragment_Stage_3440_Item_00()
EndFunction

Function Fragment_Stage_3441_Item_00()
EndFunction

Function Fragment_Stage_3442_Item_00()
EndFunction

Function Fragment_Stage_3450_Item_00()
EndFunction

Function Fragment_Stage_3451_Item_00()
EndFunction

Function Fragment_Stage_3452_Item_00()
EndFunction

Function Fragment_Stage_3460_Item_00()
EndFunction

Function Fragment_Stage_3461_Item_00()
EndFunction

Function Fragment_Stage_3462_Item_00()
EndFunction

Function Fragment_Stage_3599_Item_00()
	SetLocalPitt01Stage(3600)
EndFunction

Function Fragment_Stage_3600_Item_00()
	SetLocalPitt01Stage(4500)
EndFunction

Function Fragment_Stage_4499_Item_00()
	SetLocalPitt01Stage(4500)
EndFunction

Function Fragment_Stage_4500_Item_00()
	SetObjectiveDisplayed(4520, True)
EndFunction

Function Fragment_Stage_4505_Item_00()
	SetLocalPitt01Stage(4510)
EndFunction

Function Fragment_Stage_4510_Item_00()
	SetObjectiveDisplayed(4520, True)
EndFunction

Function Fragment_Stage_4520_Item_00()
	SetObjectiveCompleted(4520, True)
	SetObjectiveDisplayed(4530, True)
	SetObjectiveDisplayed(4545, True)
	EnableLocalCollection(Alias_Finale_UnionFighters)
	EnableLocalCollection(Alias_Finale_Trogs)
EndFunction

Function Fragment_Stage_4530_Item_00()
	SetObjectiveDisplayed(4530, True)
	SetObjectiveDisplayed(4545, True)
EndFunction

Function Fragment_Stage_4531_Item_00()
	SetObjectiveCompleted(4545, True)
EndFunction

Function Fragment_Stage_4532_Item_00()
	SetObjectiveFailed(4545, True)
EndFunction

Function Fragment_Stage_4540_Item_00()
	SetObjectiveCompleted(4530, True)
	SetObjectiveDisplayed(4540, True)
	SetObjectiveDisplayed(4541, True)
EndFunction

Function Fragment_Stage_4699_Item_00()
	SetLocalPitt01Stage(4700)
EndFunction

Function Fragment_Stage_4700_Item_00()
	SetObjectiveCompleted(4540, True)
	SetObjectiveCompleted(4541, True)
	SetObjectiveDisplayed(4710, True)
EndFunction

Function Fragment_Stage_4750_Item_00()
	DisableLocalCollection(Alias_Finale_Trogs)
EndFunction

Function Fragment_Stage_4900_Item_00()
	SetObjectiveCompleted(4875, True)
	SetObjectiveDisplayed(4880, True)
EndFunction

Function Fragment_Stage_4950_Item_00()
	SetObjectiveCompleted(4880, True)
EndFunction

Function Fragment_Stage_5200_Item_00()
	SetObjectiveCompleted(4710, True)
	SetObjectiveDisplayed(4750, True)
EndFunction

Function Fragment_Stage_5400_Item_00()
	SetObjectiveCompleted(4750, True)
EndFunction

Function Fragment_Stage_5600_Item_00()
EndFunction

Function Fragment_Stage_9000_Item_00()
	If !IsCompleted()
		CompleteQuest()
	EndIf
EndFunction

Function Fragment_Stage_10000_Item_00()
EndFunction
