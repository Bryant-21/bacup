; RD01 Enc01 EN06 Guardian. FO76 ran this encounter server-side; the converted script kept only
; declarations. Contract: bacup/docs/stub_restoration/contracts/rd01-enc01-guardian-enc05-lab-2026-09-23.md.
; Converted RD01 scripts never call Tales. State Tales reads on the boss (alias 3):
;   RD01_Enc01_IgnoreCombat_Keyword present -> intro/blast, no player damage
;   RD01_Enc01_DamageState_Keyword present  -> vulnerable, damage passes
;   neither                                 -> shielded, only the HeatShield part (PerceptionCondition) drains
; PerceptionCondition reaching 0 (OnCripple) opens the vulnerable window. A stagger component
; (alias 26 circuit) counts as broken once its ref is disabled; the shield restore re-enables it.
; Extra timer ids: 13 combat tick, 14 outro.

Event OnQuestShutdown()
	CleanupSpawned()
EndEvent

Function StartEncounter()
	CleanupSpawned()
	BossActor = Alias_Actor_Boss.GetActorReference()
	GunBase = Alias_Static_Gun_Base.GetReference()
	OuterLights = Alias_EnableMarker_OuterLights.GetReference()
	ChargingLights = Alias_EnableMarker_ChargingState.GetReference()
	UnshieldedLights = Alias_EnableMarker_UnshieldedState.GetReference()
	DamageStateTimerDuration = GlobalOr(RD01_Enc01_DamageStateTimer, 15.0)
	GetToShieldTimerDuration = GlobalOr(RD01_Enc01_GetToShieldTimer, 15.0)
	Phase = 1
	LocalShieldNodeSets = CopyShieldNodes()

	IntroTrack.Add()
	EncounterStartScene.Start()
	If OuterLights != None
		OuterLights.Enable()
		LightTurnOnSound.Play(OuterLights)
	EndIf
	Raids:RD01:Enc01:PlatformGunAnimationsScript gunAnimations = GunBase as Raids:RD01:Enc01:PlatformGunAnimationsScript
	If gunAnimations != None
		gunAnimations.AnimateOpenUpAmbush()
	EndIf
	If BossActor != None
		BossActor.AddKeyword(RD01_Enc01_IgnoreCombat_Keyword)
		BossActor.SetValue(AmbushRelease, 0.0)
		BossActor.Enable()
		BossActor.EvaluatePackage()
		RegisterForRemoteEvent(BossActor, "OnCripple")
	EndIf

	Float ambushTime = AmbushFurnTime
	If ambushTime <= 0.0
		ambushTime = 16.0
	EndIf
	StartTimer(ambushTime, IntroAmbushFurnTimerID)
EndFunction

Function ReleaseBoss()
	Actor player = Game.GetPlayer()
	IntroTrack.Remove()
	EnterShielded()
	(Alias_Triggers_PlatformSurface as Raids:RD01:Enc01:DamagingFloorTriggerScript).SetTriggersEnabled(True)
	SetFloorsFx("GoToFog")
	If BossActor != None
		BossActor.SetValue(AmbushRelease, 1.0)
		BossActor.RemoveKeyword(RD01_Enc01_IgnoreCombat_Keyword)
		BossActor.EvaluatePackage()
		BossActor.StartCombat(player)
	EndIf
	StartTimer(GlobalOr(EnrageTime, 7200.0), EnrageTimerID)
	StartTimer(GlobalOr(DamagingFloorTimeUntilNextTelegraphDuration, 10.5), DamagingFloorTimeUntilNextTelegraphID)
	StartTimer(1.0, 13)
EndFunction

Function SetPhase(Int aiPhase)
	If Phase <= 0 || aiPhase <= Phase
		Return
	EndIf
	Phase = aiPhase
	If Phase >= 2
		Alias_Actors_Turrets.EnableAll()
		If NodeSelected < 0
			Alias_Actors_Turrets.StartCombatAll(Game.GetPlayer())
		EndIf
	EndIf
	If Phase >= 3
		Raids:RD01:Enc01:PlatformGunAnimationsScript gunAnimations = GunBase as Raids:RD01:Enc01:PlatformGunAnimationsScript
		If gunAnimations != None
			gunAnimations.AnimateOpenUpGuns()
		EndIf
		SetPlatformGunsFiring(True)
	EndIf
EndFunction

Function EnterShielded()
	IsBossVulnerable = False
	If BossActor != None
		BossActor.RemoveKeyword(RD01_Enc01_DamageState_Keyword)
		BossActor.AddKeyword(UseAlternateBloodMaterialKeyword)
		BossActor.RestoreValue(PartToCrippleAV, 1000.0)
	EndIf
	SetRefEnabled(Alias_ShieldedLight.GetReference(), True)
	SetRefEnabled(UnshieldedLights, False)
	SetRefEnabled(ChargingLights, False)
	VulnerableTrack.Remove()
	InvulnerableTrack.Add()
EndFunction

Function EnterVulnerable()
	If IsBossVulnerable || NodeSelected >= 0 || BossActor == None || BossActor.IsDead() || BossActor.HasKeyword(RD01_Enc01_IgnoreCombat_Keyword)
		Return
	EndIf
	IsBossVulnerable = True
	BossActor.AddKeyword(RD01_Enc01_DamageState_Keyword)
	BossActor.RemoveKeyword(UseAlternateBloodMaterialKeyword)
	BossVulnerableMessage.Show()
	InvulnerableTrack.Remove()
	VulnerableTrack.Add()
	SetRefEnabled(Alias_ShieldedLight.GetReference(), False)
	SetRefEnabled(UnshieldedLights, True)
	StartTimer(DamageStateTimerDuration, DamageStateTimerID)
EndFunction

Event Actor.OnCripple(Actor akSender, ActorValue akActorValue, Bool abCrippled)
	If akSender == BossActor && akActorValue == PartToCrippleAV && abCrippled
		EnterVulnerable()
	EndIf
EndEvent

Function StartSuperheat()
	If BossActor == None || BossActor.IsDead()
		Return
	EndIf
	IsBossVulnerable = False
	BossActor.RemoveKeyword(RD01_Enc01_DamageState_Keyword)
	If LocalShieldNodeSets == None || LocalShieldNodeSets.Length == 0
		RestoreShelters()
		LocalShieldNodeSets = CopyShieldNodes()
	EndIf
	If LocalShieldNodeSets.Length == 0
		EnterShielded()
		Return
	EndIf
	NodeSelected = Utility.RandomInt(0, LocalShieldNodeSets.Length - 1)
	SelectedShieldNode = LocalShieldNodeSets[NodeSelected]
	LocalShieldNodeSets.Remove(NodeSelected)

	SuperheatStartMessage.Show()
	SuperheatStartScene.Start()
	VulnerableTrack.Remove()
	SetRefEnabled(UnshieldedLights, False)
	SetRefEnabled(ChargingLights, True)
	SetRefEnabled(AliasRef(SelectedShieldNode.NodeStateLightEnableMarker), True)
	SetKlaxon(AliasRef(SelectedShieldNode.NodeWarningLight), KlaxonStateOn)
	ObjectReference nodeDoor = AliasRef(SelectedShieldNode.NodeDoor)
	If nodeDoor != None
		nodeDoor.SetOpen(True)
	EndIf
	Raids:RD01:Enc01:SafeRoomTriggerVolume shelter = SelectedShieldNode.NodeTrigger as Raids:RD01:Enc01:SafeRoomTriggerVolume
	If shelter != None
		shelter.TurnOn()
	EndIf

	Float doorCloseTime = fNodeDoorCloseTime
	If doorCloseTime <= 0.0 || doorCloseTime >= GetToShieldTimerDuration
		doorCloseTime = GetToShieldTimerDuration * 0.9
	EndIf
	StartTimer(doorCloseTime, iNodeDoorCloseTimerID)
	StartTimer(GetToShieldTimerDuration, GetToShieldTimerID)
EndFunction

Function Blast()
	If SelectedShieldNode == None
		Return
	EndIf
	Actor player = Game.GetPlayer()
	If BossActor != None
		BossActor.AddKeyword(RD01_Enc01_IgnoreCombat_Keyword)
		BossActor.EvaluatePackage()
	EndIf
	SetTurretsIgnoreCombat(True)

	Raids:RD01:Enc01:SafeRoomTriggerVolume shelter = SelectedShieldNode.NodeTrigger as Raids:RD01:Enc01:SafeRoomTriggerVolume
	Bool sheltered = shelter != None && shelter.IsPlayerSheltered()
	If !sheltered && !player.IsDead()
		KillImod.Apply()
		player.Kill(BossActor)
	EndIf
	If shelter != None
		shelter.TurnOff()
	EndIf
	SetKlaxon(AliasRef(SelectedShieldNode.NodeWarningLight), KlaxonStateOff)
	SetRefEnabled(AliasRef(SelectedShieldNode.NodeStateLightEnableMarker), False)
	SetRefEnabled(ChargingLights, False)
	SuperheatEnd.Start()

	StartTimer(NodeDoorTimeUntilFlames, NodeDoorTimeUntilFlamesTimerID)
	StartTimer(NodeDoorTimeUntilBlast, NodeDoorTimeUntilBlastTimerID)
	StartTimer(fPostBlastAttackTime, iPostBlastAttackTimerID)
EndFunction

Function EndBlast()
	NodeSelected = -1
	RestoreStaggerComponents()
	If BossActor == None || BossActor.IsDead()
		Return
	EndIf
	EnterShielded()
	BossActor.RemoveKeyword(RD01_Enc01_IgnoreCombat_Keyword)
	BossActor.EvaluatePackage()
	SetTurretsIgnoreCombat(False)
	Actor player = Game.GetPlayer()
	If !player.IsDead()
		BossActor.StartCombat(player)
	EndIf
EndFunction

Function ShelterDoorFlames()
	If SelectedShieldNode == None
		Return
	EndIf
	ObjectReference nodeDoor = AliasRef(SelectedShieldNode.NodeDoor)
	If nodeDoor != None
		SetRefEnabled(nodeDoor.GetLinkedRef(LinkCustom01), True)
	EndIf
EndFunction

; The used shelter's door is blown away and that shelter stays out of LocalShieldNodeSets.
Function ShelterDoorBlast()
	If SelectedShieldNode == None
		Return
	EndIf
	ObjectReference nodeDoor = AliasRef(SelectedShieldNode.NodeDoor)
	If nodeDoor != None
		If kNodeDoorExplosion != None
			nodeDoor.PlaceAtMe(kNodeDoorExplosion)
		EndIf
		SetRefEnabled(nodeDoor.GetLinkedRef(LinkCustom01), False)
		SetRefEnabled(nodeDoor.GetLinkedRef(), False)
		nodeDoor.Disable()
	EndIf
	SelectedShieldNode = None
EndFunction

Function CloseShelterDoor()
	If SelectedShieldNode == None
		Return
	EndIf
	ObjectReference nodeDoor = AliasRef(SelectedShieldNode.NodeDoor)
	If nodeDoor != None
		nodeDoor.SetOpen(False)
	EndIf
EndFunction

; ShieldedLights holds the components already handled this shield cycle: the skeleton has no
; dedicated variable and member patches cannot declare one.
Function CheckStaggerComponents()
	If BossActor == None || BossActor.IsDead()
		Return
	EndIf
	If ShieldedLights == None
		ShieldedLights = new ObjectReference[0]
	EndIf
	Int index = 0
	While index < Alias_Statics_StaggerComponents.GetCount()
		ObjectReference circuit = Alias_Statics_StaggerComponents.GetAt(index)
		If circuit != None && circuit.IsDisabled() && ShieldedLights.Find(circuit) < 0
			ShieldedLights.Add(circuit)
			StaggerBoss()
		EndIf
		index += 1
	EndWhile
EndFunction

Function StaggerBoss()
	BossStaggeredMessage.Show()
	MeleeComponentDestroyedScene.Start()
	If MegaDamageExplosion != None
		BossActor.PlaceAtMe(MegaDamageExplosion)
	EndIf
	GlobalVariable chunk = ShieldedStateComponentDamage
	If IsBossVulnerable
		chunk = DamageStateComponentDamage
	EndIf
	If chunk != None && chunk.GetValue() > 0.0
		BossActor.DamageValue(HealthAV, chunk.GetValue())
	EndIf
	Quests:_Default:SetStageOnHealthThreshold thresholds = Alias_Actor_Boss as Quests:_Default:SetStageOnHealthThreshold
	If thresholds != None
		thresholds.EvaluateThresholds()
	EndIf
EndFunction

Function RestoreStaggerComponents()
	Int index = 0
	While index < Alias_Statics_StaggerComponents.GetCount()
		SetRefEnabled(Alias_Statics_StaggerComponents.GetAt(index), True)
		index += 1
	EndWhile
	ShieldedLights = new ObjectReference[0]
EndFunction

Function StartFloorTelegraph()
	DamagingFloorsChosen = ChooseFloors()
	NumFloorsChosen = DamagingFloorsChosen.Length
	Int index = 0
	While index < NumFloorsChosen
		ObjectReference fx = DamagingFloorsChosen[index].GetLinkedRef(LinkCustom01)
		Raids:RD01:Enc01:DamagingFloorFXScript fxScript = fx as Raids:RD01:Enc01:DamagingFloorFXScript
		If fxScript != None
			fxScript.FogToTelegraph()
		EndIf
		If fx != None
			SetRefEnabled(fx.GetLinkedRef(LinkCustom01), True)
		EndIf
		index += 1
	EndWhile
	StartTimer(GlobalOr(DamagingFloorTelegraphTimerDuration, 4.5), DamagingFloorTelegraphTimerID)
EndFunction

Function StartFloorDamage()
	Int index = 0
	While index < NumFloorsChosen
		ObjectReference floorTrigger = DamagingFloorsChosen[index]
		ObjectReference fx = floorTrigger.GetLinkedRef(LinkCustom01)
		Raids:RD01:Enc01:DamagingFloorFXScript fxScript = fx as Raids:RD01:Enc01:DamagingFloorFXScript
		If fx != None
			SetRefEnabled(fx.GetLinkedRef(LinkCustom01), False)
		EndIf
		If fxScript != None
			fxScript.TelegraphToDamage()
		EndIf
		SetRefEnabled(floorTrigger.GetLinkedRef(LinkCustom02), True)
		floorTrigger.Enable()
		index += 1
	EndWhile
	StartTimer(GlobalOr(DamagingFloorTimerDuration, 15.0), DamagingFloorTimerID)
EndFunction

Function StopFloorDamage(Bool abScheduleNext)
	(Alias_Triggers_DamagingFloors as Raids:RD01:Enc01:DamagingFloorTriggerScript).SetTriggersEnabled(False)
	Int index = 0
	While index < Alias_Triggers_DamagingFloors.GetCount()
		ObjectReference floorTrigger = Alias_Triggers_DamagingFloors.GetAt(index)
		If floorTrigger != None
			SetRefEnabled(floorTrigger.GetLinkedRef(LinkCustom02), False)
			ObjectReference fx = floorTrigger.GetLinkedRef(LinkCustom01)
			If fx != None
				SetRefEnabled(fx.GetLinkedRef(LinkCustom01), False)
			EndIf
		EndIf
		index += 1
	EndWhile
	If abScheduleNext
		SetFloorsFx("GoToFog")
		StartTimer(GlobalOr(DamagingFloorTimeUntilNextTelegraphDuration, 10.5), DamagingFloorTimeUntilNextTelegraphID)
	Else
		SetFloorsFx("FogToOff")
	EndIf
	NumFloorsChosen = 0
EndFunction

ObjectReference[] Function ChooseFloors()
	ObjectReference[] candidates = new ObjectReference[0]
	Int index = 0
	While index < Alias_Triggers_DamagingFloors.GetCount()
		ObjectReference floorTrigger = Alias_Triggers_DamagingFloors.GetAt(index)
		If floorTrigger != None
			candidates.Add(floorTrigger)
		EndIf
		index += 1
	EndWhile
	Int wanted = Phase
	If DamagingFloorsPerPhase != None && Phase >= 1 && Phase <= DamagingFloorsPerPhase.Length
		wanted = DamagingFloorsPerPhase[Phase - 1]
	EndIf
	ObjectReference[] chosen = new ObjectReference[0]
	While chosen.Length < wanted && candidates.Length > 0
		Int pick = Utility.RandomInt(0, candidates.Length - 1)
		chosen.Add(candidates[pick])
		candidates.Remove(pick)
	EndWhile
	Return chosen
EndFunction

Function SetFloorsFx(String asTransition)
	Int index = 0
	While index < Alias_Triggers_DamagingFloors.GetCount()
		ObjectReference floorTrigger = Alias_Triggers_DamagingFloors.GetAt(index)
		If floorTrigger != None
			Raids:RD01:Enc01:DamagingFloorFXScript fxScript = floorTrigger.GetLinkedRef(LinkCustom01) as Raids:RD01:Enc01:DamagingFloorFXScript
			If fxScript != None
				If asTransition == "GoToFog"
					fxScript.GoToFog()
				Else
					fxScript.FogToOff()
				EndIf
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

Function EncounterVictory()
	If Phase <= 0
		Return
	EndIf
	Phase = 0
	CancelEncounterTimers()
	StopMusic()
	SuccessTrack.Add()
	StopHazards()
	Alias_Actors_Turrets.DisableAll()
	StartTimer(OutroLength, 14)
EndFunction

; Wipe-reset entry point the raid controller calls on every module (also run on shutdown and at
; every start). Returns the arena to its pre-start state; the controller unseals the doors.
Function CleanupSpawned()
	Phase = 0
	CancelEncounterTimers()
	CancelTimer(14)
	StopMusic()
	SuccessTrack.Remove()
	EncounterStartScene.Stop()
	SuperheatStartScene.Stop()
	SuperheatEnd.Stop()
	MeleeComponentDestroyedScene.Stop()
	StopHazards()
	KillImod.Remove()
	Alias_Actors_Turrets.DisableAll()
	SetTurretsIgnoreCombat(False)
	RestoreShelters()
	RestoreStaggerComponents()
	SetRefEnabled(OuterLights, False)
	SetRefEnabled(ChargingLights, False)
	SetRefEnabled(UnshieldedLights, False)
	SetRefEnabled(Alias_ShieldedLight.GetReference(), False)
	Raids:RD01:Enc01:PlatformGunAnimationsScript gunAnimations = GunBase as Raids:RD01:Enc01:PlatformGunAnimationsScript
	If gunAnimations != None
		gunAnimations.AnimateReset()
	EndIf
	If BossActor != None
		UnregisterForRemoteEvent(BossActor, "OnCripple")
		BossActor.RemoveKeyword(RD01_Enc01_DamageState_Keyword)
		BossActor.RemoveKeyword(UseAlternateBloodMaterialKeyword)
		BossActor.RemoveKeyword(RD01_Enc01_IgnoreCombat_Keyword)
		BossActor.RemovePerk(EnragePerk)
		BossActor.StopCombat()
		If BossActor.IsDead()
			BossActor.Resurrect()
		EndIf
		BossActor.ResetHealthAndLimbs()
		BossActor.SetValue(AmbushRelease, 0.0)
		BossActor.Disable()
	EndIf
	IsBossVulnerable = False
	NodeSelected = -1
	SelectedShieldNode = None
	LocalShieldNodeSets = CopyShieldNodes()
EndFunction

Function StopHazards()
	StopFloorDamage(False)
	(Alias_Triggers_PlatformSurface as Raids:RD01:Enc01:DamagingFloorTriggerScript).SetTriggersEnabled(False)
	SetPlatformGunsFiring(False)
EndFunction

Function RestoreShelters()
	If ShieldNodeSets == None
		Return
	EndIf
	Int index = 0
	While index < ShieldNodeSets.Length
		ShieldNodeDatum node = ShieldNodeSets[index]
		If node != None
			ObjectReference nodeDoor = AliasRef(node.NodeDoor)
			If nodeDoor != None
				nodeDoor.Enable()
				SetRefEnabled(nodeDoor.GetLinkedRef(), True)
				SetRefEnabled(nodeDoor.GetLinkedRef(LinkCustom01), False)
				nodeDoor.SetOpen(True)
			EndIf
			SetRefEnabled(AliasRef(node.NodeStateLightEnableMarker), False)
			SetKlaxon(AliasRef(node.NodeWarningLight), KlaxonStateOff)
			Raids:RD01:Enc01:SafeRoomTriggerVolume shelter = node.NodeTrigger as Raids:RD01:Enc01:SafeRoomTriggerVolume
			If shelter != None
				shelter.TurnOff()
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

Function SetPlatformGunsFiring(Bool abFiring)
	Int index = 0
	While index < PlatformGuns.GetCount()
		Raids:RD01:Enc01:PlatformGunScript gun = PlatformGuns.GetAt(index) as Raids:RD01:Enc01:PlatformGunScript
		If gun != None
			If abFiring
				gun.StartFiring()
			Else
				gun.StopFiring()
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

Function SetTurretsIgnoreCombat(Bool abIgnore)
	Int index = 0
	While index < Alias_Actors_Turrets.GetCount()
		Actor turret = Alias_Actors_Turrets.GetAt(index) as Actor
		If turret != None
			If abIgnore
				turret.AddKeyword(RD01_Enc01_IgnoreCombat_Keyword)
			Else
				turret.RemoveKeyword(RD01_Enc01_IgnoreCombat_Keyword)
			EndIf
			turret.EvaluatePackage()
		EndIf
		index += 1
	EndWhile
EndFunction

Function SetKlaxon(ObjectReference akKlaxon, Int aiState)
	KlaxonManagerScript klaxon = akKlaxon as KlaxonManagerScript
	If klaxon != None && klaxon.KlaxonState != aiState
		klaxon.KlaxonState = aiState
		klaxon.NotifyKlaxonStateChanged()
	EndIf
EndFunction

Function CancelEncounterTimers()
	CancelTimer(DamageStateTimerID)
	CancelTimer(GetToShieldTimerID)
	CancelTimer(DamagingFloorTelegraphTimerID)
	CancelTimer(DamagingFloorTimerID)
	CancelTimer(DamagingFloorTimeUntilNextTelegraphID)
	CancelTimer(IntroLightTimerID)
	CancelTimer(IntroAmbushFurnTimerID)
	CancelTimer(iPostBlastAttackTimerID)
	CancelTimer(iBossStaggerTimerID)
	CancelTimer(iNodeDoorCloseTimerID)
	CancelTimer(EnrageTimerID)
	CancelTimer(NodeDoorTimeUntilFlamesTimerID)
	CancelTimer(NodeDoorTimeUntilBlastTimerID)
	CancelTimer(13)
EndFunction

Function StopMusic()
	IntroTrack.Remove()
	InvulnerableTrack.Remove()
	VulnerableTrack.Remove()
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID == 14
		SuccessTrack.Remove()
		Return
	EndIf
	If Phase <= 0
		Return
	EndIf
	If aiTimerID == IntroAmbushFurnTimerID
		ReleaseBoss()
	ElseIf aiTimerID == DamageStateTimerID
		StartSuperheat()
	ElseIf aiTimerID == iNodeDoorCloseTimerID
		CloseShelterDoor()
	ElseIf aiTimerID == GetToShieldTimerID
		Blast()
	ElseIf aiTimerID == NodeDoorTimeUntilFlamesTimerID
		ShelterDoorFlames()
	ElseIf aiTimerID == NodeDoorTimeUntilBlastTimerID
		ShelterDoorBlast()
	ElseIf aiTimerID == iPostBlastAttackTimerID
		EndBlast()
	ElseIf aiTimerID == DamagingFloorTimeUntilNextTelegraphID
		StartFloorTelegraph()
	ElseIf aiTimerID == DamagingFloorTelegraphTimerID
		StartFloorDamage()
	ElseIf aiTimerID == DamagingFloorTimerID
		StopFloorDamage(True)
	ElseIf aiTimerID == EnrageTimerID
		EnrageMessage.Show()
		If BossActor != None
			BossActor.AddPerk(EnragePerk)
		EndIf
	ElseIf aiTimerID == 13
		CheckStaggerComponents()
		If BossActor != None && !BossActor.IsDead() && BossActor.GetValue(PartToCrippleAV) <= 0.0
			EnterVulnerable()
		EndIf
		StartTimer(1.0, 13)
	EndIf
EndEvent

ShieldNodeDatum[] Function CopyShieldNodes()
	ShieldNodeDatum[] nodes = new ShieldNodeDatum[0]
	If ShieldNodeSets != None
		Int index = 0
		While index < ShieldNodeSets.Length
			If ShieldNodeSets[index] != None
				nodes.Add(ShieldNodeSets[index])
			EndIf
			index += 1
		EndWhile
	EndIf
	Return nodes
EndFunction

ObjectReference Function AliasRef(ReferenceAlias akAlias)
	If akAlias == None
		Return None
	EndIf
	Return akAlias.GetReference()
EndFunction

Function SetRefEnabled(ObjectReference akRef, Bool abEnabled)
	If akRef == None
		Return
	EndIf
	If abEnabled
		akRef.Enable()
	Else
		akRef.Disable()
	EndIf
EndFunction

Float Function GlobalOr(GlobalVariable akGlobal, Float afDefault)
	If akGlobal != None && akGlobal.GetValue() > 0.0
		Return akGlobal.GetValue()
	EndIf
	Return afDefault
EndFunction
