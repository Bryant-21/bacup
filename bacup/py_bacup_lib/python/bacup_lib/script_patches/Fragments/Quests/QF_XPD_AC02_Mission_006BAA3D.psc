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

Function SetLocalTaxStage(Int aiStage)
	If !IsStageDone(aiStage)
		SetStage(aiStage)
	EndIf
EndFunction

Function Fragment_Stage_0000_Item_00()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If ExpeditionLeader != None && ExpeditionLeader.GetReference() != playerRef
			ExpeditionLeader.ForceRefTo(playerRef)
		EndIf
		If PlayerAlias != None && PlayerAlias.GetReference() != playerRef
			PlayerAlias.ForceRefTo(playerRef)
		EndIf
		If ExpeditionTeam != None && ExpeditionTeam.Find(playerRef) < 0
			ExpeditionTeam.AddRef(playerRef)
		EndIf
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	EnableLocalAlias(Alias_EnableMarker_Manholes)
EndFunction

Function Fragment_Stage_0115_Item_00()
	If BickeringScene != None && !BickeringScene.IsPlaying()
		BickeringScene.Start()
	EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
EndFunction

Function Fragment_Stage_0200_Item_00()
	EnableLocalAlias(AmbientNPCEnableMarker)
EndFunction

Function Fragment_Stage_0215_Item_00()
	EnableLocalAlias(SalCasino)
EndFunction

Function Fragment_Stage_0245_Item_00()
	EnableLocalAlias(BillyNightClub)
EndFunction

Function Fragment_Stage_0250_Item_00()
	EnableLocalAlias(BossAlias)
EndFunction

Function Fragment_Stage_0300_Item_00()
	EnableLocalAlias(BillyNightClub)
EndFunction

Function Fragment_Stage_0345_Item_00()
	EnableLocalAlias(Sal)
EndFunction

Function Fragment_Stage_0350_Item_00()
	DisableLocalAlias(Doorman)
	UnlockLocalAlias(OfficeDoor)
	UnlockLocalCollection(DoormanDoors)
	EnableLocalAlias(CasinoAmbientCombatMarker)
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
	EnableLocalAlias(Alias_EnableMarker_Manholes)
EndFunction

Function Fragment_Stage_0605_Item_00()
EndFunction

Function Fragment_Stage_0610_Item_00()
	SetObjectiveDisplayed(620, True)
EndFunction

Function Fragment_Stage_0612_Item_00()
	SetObjectiveCompleted(620, True)
	SetObjectiveDisplayed(625, True)
EndFunction

Function Fragment_Stage_0615_Item_00()
	SetObjectiveDisplayed(625, True)
EndFunction

Function Fragment_Stage_0620_Item_00()
	SetObjectiveCompleted(620, True)
	SetObjectiveCompleted(625, True)
	SetObjectiveDisplayed(630, True)
EndFunction

Function Fragment_Stage_0800_Item_00()
	SetObjectiveCompleted(610, True)
	SetObjectiveCompleted(630, True)
	SetLocalTaxStage(1000)
EndFunction

Function Fragment_Stage_0899_Item_00()
	SetLocalTaxStage(1000)
EndFunction

Function Fragment_Stage_1000_Item_00()
EndFunction

Function Fragment_Stage_1010_Item_00()
EndFunction

Function Fragment_Stage_1020_Item_00()
	EnableLocalCollection(MunisToEnable)
EndFunction

Function Fragment_Stage_1025_Item_00()
	EnableLocalAlias(CasinoAmbientCombatMarker)
EndFunction

Function Fragment_Stage_1030_Item_00()
EndFunction

Function Fragment_Stage_1031_Item_00()
EndFunction

Function Fragment_Stage_1032_Item_00()
EndFunction

Function Fragment_Stage_1033_Item_00()
EndFunction

Function Fragment_Stage_1034_Item_00()
EndFunction

Function Fragment_Stage_1035_Item_00()
EndFunction

Function Fragment_Stage_1036_Item_00()
EndFunction

Function Fragment_Stage_1037_Item_00()
EndFunction

Function Fragment_Stage_1038_Item_00()
EndFunction

Function Fragment_Stage_1040_Item_00()
	EnableLocalAlias(TruckLeftFill)
EndFunction

Function Fragment_Stage_1045_Item_00()
	EnableLocalAlias(TruckRightFill)
EndFunction

Function Fragment_Stage_1050_Item_00()
	EnableLocalAlias(TruckCenterFill)
EndFunction

Function Fragment_Stage_1599_Item_00()
	SetLocalTaxStage(1600)
EndFunction

Function Fragment_Stage_1600_Item_00()
	SetObjectiveDisplayed(1040, True)
EndFunction

Function Fragment_Stage_1650_Item_00()
	DisableLocalAlias(Alias_AttackEWSModule)
EndFunction

Function Fragment_Stage_1700_Item_00()
	SetObjectiveCompleted(40, True)
EndFunction

Function Fragment_Stage_1725_Item_00()
EndFunction

Function Fragment_Stage_1750_Item_00()
	SetObjectiveCompleted(1040, True)
	SetObjectiveDisplayed(1050, True)
EndFunction

Function Fragment_Stage_1775_Item_00()
	SetObjectiveCompleted(1050, True)
EndFunction

Function Fragment_Stage_1800_Item_00()
	SetObjectiveDisplayed(1630, True)
	SetObjectiveDisplayed(1640, True)
	SetObjectiveDisplayed(1651, True)
	SetObjectiveDisplayed(1654, True)
EndFunction

Function Fragment_Stage_1899_Item_00()
	SetLocalTaxStage(2000)
EndFunction

Function Fragment_Stage_2000_Item_00()
EndFunction

Function Fragment_Stage_2100_Item_00()
EndFunction

Function Fragment_Stage_2599_Item_00()
	SetLocalTaxStage(2600)
EndFunction

Function Fragment_Stage_2600_Item_00()
	SetObjectiveDisplayed(1610, True)
EndFunction

Function Fragment_Stage_2650_Item_00()
	SetObjectiveCompleted(1654, True)
EndFunction

Function Fragment_Stage_2700_Item_00()
EndFunction

Function Fragment_Stage_2899_Item_00()
	SetLocalTaxStage(3000)
EndFunction

Function Fragment_Stage_3000_Item_00()
EndFunction

Function Fragment_Stage_3010_Item_00()
EndFunction

Function Fragment_Stage_3150_Item_00()
	SetObjectiveFailed(1654, True)
EndFunction

Function Fragment_Stage_3500_Item_00()
	SetObjectiveFailed(1654, True)
EndFunction

Function Fragment_Stage_3599_Item_00()
	SetLocalTaxStage(3600)
EndFunction

Function Fragment_Stage_3600_Item_00()
	SetLocalTaxStage(4500)
EndFunction

Function AddLocalSlot(ReferenceAlias akSlot)
	EnableLocalAlias(akSlot)
	If akSlot != None && akSlot.GetReference() != None && SalSlots != None && SalSlots.Find(akSlot.GetReference()) < 0
		SalSlots.AddRef(akSlot.GetReference())
	EndIf
EndFunction

Function Fragment_Stage_3651_Item_00()
	AddLocalSlot(Slot1)
EndFunction

Function Fragment_Stage_3652_Item_00()
	AddLocalSlot(Slot2)
EndFunction

Function Fragment_Stage_3653_Item_00()
	AddLocalSlot(Slot3)
EndFunction

Function Fragment_Stage_3654_Item_00()
	AddLocalSlot(Slot4)
EndFunction

Function Fragment_Stage_3655_Item_00()
	AddLocalSlot(Slot5)
EndFunction

Function Fragment_Stage_3656_Item_00()
	AddLocalSlot(Slot6)
EndFunction

Function Fragment_Stage_3657_Item_00()
	AddLocalSlot(Slot7)
EndFunction

Function Fragment_Stage_3658_Item_00()
	AddLocalSlot(Slot8)
EndFunction

Function Fragment_Stage_3659_Item_00()
	AddLocalSlot(Slot9)
EndFunction

Function Fragment_Stage_3670_Item_00()
	AddLocalSlot(Slot10)
EndFunction

Function Fragment_Stage_3671_Item_00()
	AddLocalSlot(Slot11)
EndFunction

Function Fragment_Stage_3672_Item_00()
	AddLocalSlot(Slot12)
EndFunction

Function Fragment_Stage_3673_Item_00()
	AddLocalSlot(Slot13)
EndFunction

Function Fragment_Stage_3674_Item_00()
	AddLocalSlot(Slot14)
EndFunction

Function Fragment_Stage_3680_Item_00()
EndFunction

Function Fragment_Stage_3681_Item_00()
	EnableLocalAlias(CheaterCorpse01)
EndFunction

Function Fragment_Stage_3682_Item_00()
	EnableLocalAlias(CheaterCorpse02)
EndFunction

Function Fragment_Stage_3683_Item_00()
	EnableLocalAlias(CheaterCorpse03)
EndFunction

Function Fragment_Stage_3684_Item_00()
	EnableLocalAlias(CheaterCorpse04)
EndFunction

Function Fragment_Stage_3690_Item_00()
	SetObjectiveCompleted(1652, True)
	SetObjectiveDisplayed(1653, True)
EndFunction

Function Fragment_Stage_3700_Item_00()
	SetObjectiveCompleted(1653, True)
EndFunction

Function Fragment_Stage_4499_Item_00()
	SetLocalTaxStage(4500)
EndFunction

Function Fragment_Stage_4500_Item_00()
	SetObjectiveDisplayed(30, True)
	EnableLocalAlias(BossAlias)
EndFunction

Function Fragment_Stage_4510_Item_00()
	SetObjectiveDisplayed(1610, True)
EndFunction

Function Fragment_Stage_4515_Item_00()
EndFunction

Function Fragment_Stage_4530_Item_00()
	EnableLocalCollection(AllAuditorsBossFight)
	EnableLocalCollection(AuditorsFirstGroup)
	EnableLocalCollection(AuditorsSecondGroup)
	EnableLocalCollection(AuditorsThirdGroup)
	EnableLocalCollection(AuditorsFourthGroup)
EndFunction

Function Fragment_Stage_4699_Item_00()
	SetLocalTaxStage(4700)
EndFunction

Function Fragment_Stage_4700_Item_00()
	SetObjectiveCompleted(30, True)
	SetObjectiveDisplayed(40, True)
EndFunction

Function Fragment_Stage_4710_Item_00()
	SetObjectiveCompleted(10, True)
EndFunction

Function Fragment_Stage_4720_Item_00()
	DisableLocalAlias(BossAlias)
EndFunction

Function Fragment_Stage_5000_Item_00()
	SetObjectiveCompleted(1640, True)
	SetObjectiveDisplayed(1650, True)
EndFunction

Function Fragment_Stage_5010_Item_00()
	SetObjectiveCompleted(1650, True)
EndFunction

Function Fragment_Stage_5100_Item_00()
	SetObjectiveDisplayed(40, True)
EndFunction

Function Fragment_Stage_5200_Item_00()
	SetObjectiveCompleted(40, True)
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
