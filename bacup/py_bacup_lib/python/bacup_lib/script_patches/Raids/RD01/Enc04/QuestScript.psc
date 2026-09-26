; FO76 ran Squad Epsilon server-side; the client script is declaration-only. Local
; single-player loop per bacup/docs/stub_restoration/contracts/
; rd01-enc02-drill-enc04-squad-2026-09-23.md §2.2. Phase: 0 idle, 1 intro, 2 fight,
; 3 outro, 4 finished. The module only sets 9000; the raid controller owns 10000 and
; the master stages.
;
; The shield generators are STATs (FO4 dropped their DEST). A generator is online
; while its ref is enabled: whatever makes it breakable (Tales' stand-in actor)
; disables the ref when it is destroyed, and an eyebot repair re-enables it.

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == 100
		BeginEncounter()
	ElseIf auiStageID == CompletionStage
		Phase = 4
		CancelEncounterTimers()
		DeleteEyebots()
		IntroTrack.Remove()
		CombatTrack.Remove()
		SuccessTrack.Add()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == GeneratorPollTimerID() && Phase == 2
		RefreshGenerators()
		StartTimer(1.0, GeneratorPollTimerID())
	ElseIf aiTimerID == GrenadierSpawnTimerID && Phase == 2
		ReleaseBoss(Grenadier, Alias_Furniture_Grenadier, Alias_Grenadier_Marker, GrenadierSpawnScene)
	ElseIf aiTimerID == AssassinSpawnTimerID && Phase == 2
		ReleaseBoss(Assassin, Alias_Furniture_Assassin, Alias_Assassin_Marker, AssassinSpawnScene)
	ElseIf aiTimerID == EyebotSpawnTimerID && Phase == 2
		If CountLivingEyebots() < MaxEyebots.GetValueInt()
			ObjectReference generator = FindGeneratorNeedingRepair()
			If generator != None
				SpawnEyebot(generator)
			EndIf
		EndIf
		StartTimer(EyebotSpawnInterval.GetValue(), EyebotSpawnTimerID)
	ElseIf aiTimerID == IntroMusicTimerID && Phase == 2
		IntroTrack.Remove()
		CombatTrack.Add()
	ElseIf aiTimerID == GeneratorsDownVOTimerID
		bGeneratorsDownVOAvailable = True
	ElseIf aiTimerID == GeneratorsRepairedVOTimerID
		bGeneratorsRepairedVOAvailable = True
	EndIf
EndEvent

Event ReferenceAlias.OnDeath(ReferenceAlias akSender, Actor akKiller)
	If Phase != 2
		Return
	EndIf
	If CountDeadBosses() >= 3
		Phase = 3
		CancelEncounterTimers()
		DeleteEyebots()
		Utility.Wait(OutroLength)
		SetStage(CompletionStage)
	EndIf
EndEvent

Function BeginEncounter()
	CleanupSpawned()
	Brute = Alias_Brute.GetActorReference()
	Grenadier = Alias_Grenadier.GetActorReference()
	Assassin = Alias_Assassin.GetActorReference()
	RegisterForRemoteEvent(Alias_Brute, "OnDeath")
	RegisterForRemoteEvent(Alias_Grenadier, "OnDeath")
	RegisterForRemoteEvent(Alias_Assassin, "OnDeath")
	NumActiveGenerators = -1
	Phase = 2
	RefreshGenerators()
	IntroTrack.Add()
	StartTimer(IntroTrackLength as Float, IntroMusicTimerID)
	ReleaseBoss(Brute, Alias_Furniture_Brute, Alias_Brute_Marker, BruteSpawnScene)
	StartTimer(SpawnTimer_Grenadier, GrenadierSpawnTimerID)
	StartTimer(SpawnTimer_Assassin, AssassinSpawnTimerID)
	StartTimer(EyebotSpawnInterval.GetValue(), EyebotSpawnTimerID)
	StartTimer(1.0, GeneratorPollTimerID())
EndFunction

; PA wearers cannot use the hit-squad tank furniture in FO4, so the tank only plays
; its open animation and the boss is moved to its release marker. AmbushRelease is
; raised before Enable so an ambush-snap script on the boss leaves it alone.
Function ReleaseBoss(Actor akBoss, ReferenceAlias akTank, ReferenceAlias akMarker, Scene akSpawnScene)
	If akBoss == None || akBoss.IsDead()
		Return
	EndIf
	ObjectReference tank = akTank.GetReference()
	If tank != None
		tank.PlayAnimation("JumpState02")
	EndIf
	akBoss.SetValue(AmbushRelease, 1.0)
	ObjectReference marker = akMarker.GetReference()
	If marker != None
		akBoss.MoveTo(marker)
	EndIf
	akBoss.Enable()
	ApplyBossShield(akBoss)
	If akSpawnScene != None
		akSpawnScene.Start()
	EndIf
	akBoss.StartCombat(Game.GetPlayer())
EndFunction

Function ResetBoss(Actor akBoss, ReferenceAlias akTank)
	If akBoss != None
		If akBoss.IsDead()
			akBoss.Resurrect()
		EndIf
		akBoss.RemovePerk(Invulnerability_Perk)
		akBoss.RemoveSpell(BossShaderSpell)
		akBoss.SetValue(AmbushRelease, 0.0)
		akBoss.Disable()
	EndIf
	ObjectReference tank = akTank.GetReference()
	If tank != None
		tank.PlayAnimation("JumpState01")
	EndIf
EndFunction

Function ApplyBossShield(Actor akBoss)
	If akBoss == None || akBoss.IsDead() || akBoss.IsDisabled()
		Return
	EndIf
	If NumActiveGenerators > 0
		If !akBoss.HasPerk(Invulnerability_Perk)
			akBoss.AddPerk(Invulnerability_Perk)
		EndIf
		If !akBoss.HasSpell(BossShaderSpell)
			akBoss.AddSpell(BossShaderSpell, False)
		EndIf
	Else
		akBoss.RemovePerk(Invulnerability_Perk)
		akBoss.RemoveSpell(BossShaderSpell)
	EndIf
EndFunction

Function ApplyShieldToAllBosses()
	ApplyBossShield(Brute)
	ApplyBossShield(Grenadier)
	ApplyBossShield(Assassin)
EndFunction

Function RefreshGenerators()
	Int active = 0
	Int index = 0
	While index < Alias_Generators.GetCount()
		ObjectReference generator = Alias_Generators.GetAt(index)
		Bool online = generator != None && !generator.IsDisabled()
		If online
			active += 1
		EndIf
		SetKlaxon(generator, online)
		index += 1
	EndWhile
	If active == NumActiveGenerators
		Return
	EndIf
	Int previous = NumActiveGenerators
	NumActiveGenerators = active
	ApplyShieldToAllBosses()
	If active == 0 && previous > 0 && bGeneratorsDownVOAvailable
		bGeneratorsDownVOAvailable = False
		SayFromLivingBoss(GeneratorsDownTopic)
		StartTimer(fGeneratorsDownVOCooldown, GeneratorsDownVOTimerID)
	EndIf
EndFunction

Function SetKlaxon(ObjectReference akGenerator, Bool abOnline)
	If akGenerator == None
		Return
	EndIf
	KlaxonManagerScript klaxon = akGenerator.GetLinkedRef(LinkCustom01) as KlaxonManagerScript
	If klaxon == None
		Return
	EndIf
	Int wanted = KlaxonStateOff
	If abOnline
		wanted = KlaxonStateOn
	EndIf
	If klaxon.KlaxonState != wanted
		klaxon.KlaxonState = wanted
		klaxon.NotifyKlaxonStateChanged()
	EndIf
EndFunction

ObjectReference Function FindGeneratorNeedingRepair()
	Int index = 0
	While index < Alias_Generators.GetCount()
		ObjectReference generator = Alias_Generators.GetAt(index)
		If generator != None && generator.IsDisabled()
			Return generator
		EndIf
		index += 1
	EndWhile
	Return None
EndFunction

; Called by an eyebot that finished repairing a destroyed generator.
Function RestoreGenerator(ObjectReference akGenerator)
	If akGenerator == None || Phase != 2 || !akGenerator.IsDisabled()
		Return
	EndIf
	akGenerator.Enable()
	RefreshGenerators()
	If bGeneratorsRepairedVOAvailable
		bGeneratorsRepairedVOAvailable = False
		SayFromLivingBoss(GeneratorsRepairedTopic)
		StartTimer(fGeneratorsRepairedVOCooldown, GeneratorsRepairedVOTimerID)
	EndIf
EndFunction

Function SpawnEyebot(ObjectReference akGenerator)
	Int markerCount = Alias_Eyebots_Markers.GetCount()
	If markerCount < 1
		Return
	EndIf
	ObjectReference marker = Alias_Eyebots_Markers.GetAt(Utility.RandomInt(0, markerCount - 1))
	If marker == None
		Return
	EndIf
	Actor eyebot = marker.PlaceActorAtMe(ActorBase_Eyebot)
	If eyebot == None
		Return
	EndIf
	Alias_Eyebots.AddRef(eyebot)
	Raids:RD01:Enc04:EyebotRepairScript repairBot = eyebot as Raids:RD01:Enc04:EyebotRepairScript
	If repairBot != None
		repairBot.BeginRepair(akGenerator)
	EndIf
EndFunction

Int Function CountLivingEyebots()
	Int living = 0
	Int index = 0
	While index < Alias_Eyebots.GetCount()
		Actor eyebot = Alias_Eyebots.GetAt(index) as Actor
		If eyebot != None && !eyebot.IsDead()
			living += 1
		EndIf
		index += 1
	EndWhile
	Return living
EndFunction

Function DeleteEyebots()
	Int index = Alias_Eyebots.GetCount() - 1
	While index >= 0
		ObjectReference eyebot = Alias_Eyebots.GetAt(index)
		If eyebot != None
			eyebot.Disable()
			eyebot.Delete()
		EndIf
		index -= 1
	EndWhile
	Alias_Eyebots.RemoveAll()
EndFunction

Function SayFromLivingBoss(Topic akTopic)
	If akTopic == None
		Return
	EndIf
	If Brute != None && !Brute.IsDead() && !Brute.IsDisabled()
		Brute.Say(akTopic)
	ElseIf Grenadier != None && !Grenadier.IsDead() && !Grenadier.IsDisabled()
		Grenadier.Say(akTopic)
	ElseIf Assassin != None && !Assassin.IsDead() && !Assassin.IsDisabled()
		Assassin.Say(akTopic)
	EndIf
EndFunction

Int Function CountDeadBosses()
	NumDeadBosses = 0
	If Brute != None && Brute.IsDead()
		NumDeadBosses += 1
	EndIf
	If Grenadier != None && Grenadier.IsDead()
		NumDeadBosses += 1
	EndIf
	If Assassin != None && Assassin.IsDead()
		NumDeadBosses += 1
	EndIf
	Return NumDeadBosses
EndFunction

Function CancelEncounterTimers()
	CancelTimer(GrenadierSpawnTimerID)
	CancelTimer(AssassinSpawnTimerID)
	CancelTimer(EyebotSpawnTimerID)
	CancelTimer(IntroMusicTimerID)
	CancelTimer(GeneratorPollTimerID())
EndFunction

; The declared timer IDs 0-5 are all taken by the FO76 skeleton.
Int Function GeneratorPollTimerID()
	Return 6
EndFunction

; Wipe/retry reset. The raid controller calls this before Stop(); stage 100 also
; runs it so a restart is clean without Tales.
Function CleanupSpawned()
	Phase = 0
	CancelEncounterTimers()
	UnregisterForRemoteEvent(Alias_Brute, "OnDeath")
	UnregisterForRemoteEvent(Alias_Grenadier, "OnDeath")
	UnregisterForRemoteEvent(Alias_Assassin, "OnDeath")
	DeleteEyebots()
	Int index = 0
	While index < Alias_Generators.GetCount()
		ObjectReference generator = Alias_Generators.GetAt(index)
		If generator != None
			generator.Enable()
		EndIf
		SetKlaxon(generator, True)
		index += 1
	EndWhile
	ResetBoss(Alias_Brute.GetActorReference(), Alias_Furniture_Brute)
	ResetBoss(Alias_Grenadier.GetActorReference(), Alias_Furniture_Grenadier)
	ResetBoss(Alias_Assassin.GetActorReference(), Alias_Furniture_Assassin)
	NumActiveGenerators = 3
	NumDeadBosses = 0
	bGeneratorsDownVOAvailable = True
	bGeneratorsRepairedVOAvailable = True
	IntroTrack.Remove()
	CombatTrack.Remove()
	SuccessTrack.Remove()
	FailureTrack.Remove()
EndFunction
