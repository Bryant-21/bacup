; RD01 Enc06 Ultracite Terror. FO76 ran this encounter server-side, so the converted script only
; kept its declarations. Contract: bacup/docs/stub_restoration/contracts/
; rd01-enc06-ultracite-terror-2026-09-23.md. Converted RD01 scripts never call Tales: Tales reads
; stages 100/200/300/400/9000 and the tail alias (Alias_Tail, alias 36).

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == 100
		BeginEncounter()
	ElseIf auiStageID == Phase2Stage
		EnterPhase(Phase2)
	ElseIf auiStageID == Phase3Stage
		EnterPhase(Phase3)
	ElseIf auiStageID == Phase4Stage
		BeginOutro()
	ElseIf auiStageID == 9000
		HandleEncounterSuccess()
	EndIf
EndEvent

Event OnQuestShutdown()
	CleanupSpawned()
EndEvent

; Timer ids beyond the four FO76 constants: 4 outro failure, 5 head animation watchdog,
; 6 tail animation watchdog, 7 head drift check, 8 lava pillar fallback, 9 stagger recovery,
; 10 dead tail removal, 11 outro blast watchdog. The watchdogs cover a refused PlayIdle or a
; graph event lost to a save/load, so the fight never stalls waiting for an animation.
Event OnTimer(Int aiTimerID)
	If aiTimerID == HeadRelocationTimerID
		BeginHeadDescend()
	ElseIf aiTimerID == TailSpawnTimerID
		OnTailSpawnTimer()
	ElseIf aiTimerID == TailAttackTimerID
		BeginTailAttack()
	ElseIf aiTimerID == ChargeMessageTimerID
		If Phase == PhaseOutro && ChargeMessage != None
			ChargeMessage.Show()
		EndIf
	ElseIf aiTimerID == 4
		FailOutro()
	ElseIf aiTimerID == 5
		OnHeadAnimationDone()
	ElseIf aiTimerID == 6
		OnTailAnimationDone()
	ElseIf aiTimerID == 7
		CheckHeadDrift()
	ElseIf aiTimerID == 8
		If Phase == Phase3
			EruptLavaPillars(2)
			StartTimer(15.0, 8)
		EndIf
	ElseIf aiTimerID == 9
		If HeadActor != None && !HeadActor.IsDead() && Phase <= PhaseOutro
			HeadActor.AddPerk(NoStaggerPerk)
		EndIf
	ElseIf aiTimerID == 10
		If TailActor != None && TailActor.IsDead()
			DespawnTail()
			ArmTailSpawn()
		EndIf
	ElseIf aiTimerID == 11
		OutroBlast()
	EndIf
EndEvent

Event OnAnimationEvent(ObjectReference akSource, String asEventName)
	If HeadActor != None && akSource == HeadActor
		If asEventName == AnimDoneEvent
			OnHeadAnimationDone()
		ElseIf asEventName == MeleePreHitFrameEvent
			If Phase == Phase3
				EruptLavaPillars(2)
			EndIf
		ElseIf asEventName == OutroAttackExplosionEvent
			OutroBlast()
		EndIf
	ElseIf TailActor != None && akSource == TailActor && !TailActor.IsDead()
		If asEventName == TailAttackHitEvent
			If TailState == TailStateAttacking && !TailOutcomeProcessed
				SmashTailQuarter()
			EndIf
		ElseIf asEventName == AnimDoneEvent
			OnTailAnimationDone()
		EndIf
	EndIf
EndEvent

Event Actor.OnDying(Actor akSender, Actor akKiller)
	If TailActor == None || akSender != TailActor
		Return
	EndIf
	CancelTimer(TailAttackTimerID)
	CancelTimer(6)
	If !TailOutcomeProcessed
		TailOutcomeProcessed = True
		StaggerHead()
	EndIf
	StartTimer(5.0, 10)
EndEvent

Function BeginEncounter()
	CacheArenaMarkers()
	HeadActor = Alias_Head.GetActorReference()
	CleanupSpawned()
	If HeadActor == None
		Debug.Trace(Self + " RD01 Enc06: Alias_Head is empty; encounter cannot start")
		Return
	EndIf

	ObjectReference walkway = Alias_WalkwayEnableMarker.GetReference()
	If walkway != None
		walkway.Disable()
	EndIf
	PlayPhaseTrack()

	ObjectReference ambushSpot = Alias_AmbushFurn.GetReference()
	If ambushSpot == None
		ambushSpot = HeadActor
	EndIf
	CurrentHeadMarker = NearestHeadMarker(ambushSpot)

	; Solo path: skip the FO76 ambush furniture and rise from the head marker instead.
	HeadActor.SetValue(AmbushRelease, 1.0)
	HeadActor.AddKeyword(RD01_Enc06_IgnoreCombat_Keyword)
	HeadActor.AddPerk(NoStaggerPerk)
	HeadActor.SetGhost(True)
	HeadActor.Enable()
	WaitFor3D(HeadActor)
	If CurrentHeadMarker != None
		HeadActor.MoveTo(CurrentHeadMarker)
	EndIf
	BeginHeadAscend()
EndFunction

; Wipe-reset entry point for the raid controller (also run on shutdown and at every start):
; removes the tail, stops timers and music, restores all four island quarters with their lava
; pillar slices, lowers the body, re-opens the walkway and puts the head back, disabled.
Function CleanupSpawned()
	CancelAllTimers()
	DespawnTail()
	UnregisterHeadAnimationEvents()
	StopMusic()

	If HeadActor != None
		HeadActor.SetGhost(False)
		HeadActor.RemoveKeyword(RD01_Enc06_IgnoreCombat_Keyword)
		HeadActor.RemovePerk(OutroDamageMultPerk)
		HeadActor.RemovePerk(NoStaggerPerk)
		If !HeadActor.IsDead()
			HeadActor.SetRestrained(False)
		EndIf
		HeadActor.SetValue(AmbushRelease, 0.0)
		HeadActor.Disable()
	EndIf

	If TailMarkers != None
		Int i = 0
		While i < TailMarkers.Length
			ObjectReference marker = TailMarkers[i]
			If marker != None
				Raids:RD01:Enc06:PlatformDestructionScript platform = PlatformForTailMarker(marker)
				If platform != None
					platform.AnimateReset()
				EndIf
				ObjectReference slice = marker.GetLinkedRef()
				If slice != None
					slice.Enable()
				EndIf
			EndIf
			i += 1
		EndWhile
	EndIf
	LowerBody()

	ObjectReference walkway = Alias_WalkwayEnableMarker.GetReference()
	If walkway != None
		walkway.Enable()
	EndIf

	Phase = PhaseIntro
	HeadState = HeadStateIdle
	TailState = TailStateAscending
	OutroProcessed = False
	TailOutcomeProcessed = False
EndFunction

Function CancelAllTimers()
	Int timerID = 0
	While timerID <= 11
		CancelTimer(timerID)
		timerID += 1
	EndWhile
EndFunction

Function EnterPhase(Int aiPhase)
	If aiPhase <= Phase || Phase >= PhaseOutro
		Return
	EndIf
	Int oldPhase = Phase
	Phase = aiPhase
	PlayPhaseTrack()
	If oldPhase < Phase2 && Phase >= Phase2
		ArmTailSpawn()
	EndIf
	If oldPhase < Phase3 && Phase >= Phase3
		StartTimer(15.0, 8)
	EndIf
EndFunction

Function PlayPhaseTrack()
	StopMusic()
	If Phase == PhaseIntro
		IntroTrack.Add()
	ElseIf Phase == Phase1
		Phase1Track.Add()
	ElseIf Phase == Phase2
		Phase2Track.Add()
	ElseIf Phase == Phase3 || Phase == PhaseOutro
		Phase3Track.Add()
	ElseIf Phase == PhaseSuccess
		SuccessTrack.Add()
	ElseIf Phase == PhaseFailure
		FailureTrack.Add()
	EndIf
EndFunction

Function StopMusic()
	IntroTrack.Remove()
	Phase1Track.Remove()
	Phase2Track.Remove()
	Phase3Track.Remove()
	SuccessTrack.Remove()
	FailureTrack.Remove()
EndFunction

Function CacheArenaMarkers()
	If Alias_Markers_Head.GetCount() > 0
		HeadMarkers = CollectRefs(Alias_Markers_Head)
	EndIf
	If Alias_Markers_Tail.GetCount() > 0
		TailMarkers = CollectRefs(Alias_Markers_Tail)
	EndIf
EndFunction

ObjectReference[] Function CollectRefs(RefCollectionAlias akCollection)
	Int count = akCollection.GetCount()
	ObjectReference[] refs = new ObjectReference[count]
	Int i = 0
	While i < count
		refs[i] = akCollection.GetAt(i)
		i += 1
	EndWhile
	Return refs
EndFunction

Function WaitFor3D(ObjectReference akRef)
	Int tries = 0
	While akRef != None && !akRef.Is3DLoaded() && tries < 30
		Utility.Wait(0.1)
		tries += 1
	EndWhile
EndFunction

Function RaiseBody()
	Int i = 0
	While i < Alias_BodyParts.GetCount()
		Raids:RD01:Enc06:ScorchtongueBodyAnimationScript body = Alias_BodyParts.GetAt(i) as Raids:RD01:Enc06:ScorchtongueBodyAnimationScript
		If body != None
			body.CallFunctionNoWait("Rise", new Var[0])
		EndIf
		i += 1
	EndWhile
EndFunction

Function LowerBody()
	Int i = 0
	While i < Alias_BodyParts.GetCount()
		Raids:RD01:Enc06:ScorchtongueBodyAnimationScript body = Alias_BodyParts.GetAt(i) as Raids:RD01:Enc06:ScorchtongueBodyAnimationScript
		If body != None
			body.CallFunctionNoWait("Lower", new Var[0])
		EndIf
		i += 1
	EndWhile
EndFunction

; ---- Head: rise, fight from a marker, sink and relocate ----

Function EnsureHeadAnimationEvents()
	If HeadActor != None
		RegisterForAnimationEvent(HeadActor, AnimDoneEvent)
		RegisterForAnimationEvent(HeadActor, MeleePreHitFrameEvent)
		RegisterForAnimationEvent(HeadActor, OutroAttackExplosionEvent)
	EndIf
EndFunction

Function UnregisterHeadAnimationEvents()
	If HeadActor != None
		UnregisterForAnimationEvent(HeadActor, AnimDoneEvent)
		UnregisterForAnimationEvent(HeadActor, MeleePreHitFrameEvent)
		UnregisterForAnimationEvent(HeadActor, OutroAttackExplosionEvent)
	EndIf
EndFunction

Function BeginHeadAscend()
	HeadState = HeadStateAscending
	RaiseBody()
	EnsureHeadAnimationEvents()
	HeadActor.PlayIdle(AscendIdle)
	StartTimer(6.0, 5)
EndFunction

Function BeginHeadDescend()
	If HeadActor == None || HeadActor.IsDead() || HeadState != HeadStateIdle
		Return
	EndIf
	If Phase < Phase1 || Phase >= PhaseOutro
		Return
	EndIf
	CancelTimer(7)
	HeadState = HeadStateDescending
	HeadActor.AddKeyword(RD01_Enc06_IgnoreCombat_Keyword)
	EnsureHeadAnimationEvents()
	HeadActor.PlayIdle(DescendIdle)
	StartTimer(6.0, 5)
EndFunction

Function OnHeadAnimationDone()
	CancelTimer(5)
	If HeadActor == None
		Return
	EndIf
	If HeadState == HeadStateDescending
		HeadSubmerged()
	ElseIf HeadState == HeadStateAscending
		HeadSurfaced()
	EndIf
EndFunction

Function HeadSubmerged()
	HeadActor.SetGhost(True)
	LowerBody()
	ObjectReference marker = PickNextHeadMarker()
	If marker != None
		HeadActor.MoveTo(marker)
		CurrentHeadMarker = marker
	EndIf
	BeginHeadAscend()
EndFunction

Function HeadSurfaced()
	HeadState = HeadStateIdle
	HeadActor.SetGhost(False)
	HeadActor.RemoveKeyword(RD01_Enc06_IgnoreCombat_Keyword)
	If Phase == PhaseIntro
		EnterPhase(Phase1)
	EndIf
	If Phase < PhaseOutro
		StartTimer(HeadRelocationInterval.GetValue(), HeadRelocationTimerID)
	EndIf
	StartTimer(2.0, 7)
EndFunction

; The head race has no locomotion clips and no Immobile flag. Restrain it only once it is seen
; drifting off its marker, so an unrestrained head that stays put keeps its full combat AI.
Function CheckHeadDrift()
	If HeadActor == None || HeadActor.IsDead() || Phase > PhaseOutro
		Return
	EndIf
	EnsureHeadAnimationEvents()
	If HeadState == HeadStateIdle && CurrentHeadMarker != None && HeadActor.GetDistance(CurrentHeadMarker) > 256.0
		HeadActor.SetRestrained(True)
		HeadActor.MoveTo(CurrentHeadMarker)
	EndIf
	StartTimer(2.0, 7)
EndFunction

ObjectReference Function NearestHeadMarker(ObjectReference akOrigin)
	If HeadMarkers == None || akOrigin == None
		Return None
	EndIf
	ObjectReference nearest = None
	Float nearestDistance = 0.0
	Int i = 0
	While i < HeadMarkers.Length
		If HeadMarkers[i] != None
			Float distance = HeadMarkers[i].GetDistance(akOrigin)
			If nearest == None || distance < nearestDistance
				nearest = HeadMarkers[i]
				nearestDistance = distance
			EndIf
		EndIf
		i += 1
	EndWhile
	Return nearest
EndFunction

ObjectReference Function PickNextHeadMarker()
	If HeadMarkers == None
		Return None
	EndIf
	Int candidates = 0
	Int i = 0
	While i < HeadMarkers.Length
		If HeadMarkers[i] != None && HeadMarkers[i] != CurrentHeadMarker
			candidates += 1
		EndIf
		i += 1
	EndWhile
	If candidates == 0
		Return CurrentHeadMarker
	EndIf
	Int pick = Utility.RandomInt(0, candidates - 1)
	i = 0
	While i < HeadMarkers.Length
		If HeadMarkers[i] != None && HeadMarkers[i] != CurrentHeadMarker
			If pick == 0
				Return HeadMarkers[i]
			EndIf
			pick -= 1
		EndIf
		i += 1
	EndWhile
	Return CurrentHeadMarker
EndFunction

; NoStaggerPerk is on the head for the whole fight; killing the tail lifts it for one stagger.
Function StaggerHead()
	If HeadActor == None || HeadActor.IsDead() || HeadState != HeadStateIdle || Phase >= PhaseOutro
		Return
	EndIf
	HeadActor.RemovePerk(NoStaggerPerk)
	StaggerSpell.Cast(HeadActor, HeadActor)
	StartTimer(4.0, 9)
EndFunction

; ---- Tail: rise at a tail marker, smash that marker's island quarter unless killed first ----

Function ArmTailSpawn()
	If Phase >= Phase2 && Phase < PhaseOutro
		StartTimer(TailSpawnInterval.GetValue(), TailSpawnTimerID)
	EndIf
EndFunction

Function OnTailSpawnTimer()
	If TailActor == None && Phase >= Phase2 && Phase < PhaseOutro
		SpawnTail()
	EndIf
	If TailActor == None
		ArmTailSpawn()
	EndIf
EndFunction

Function SpawnTail()
	ObjectReference marker = PickTailMarker()
	If marker == None
		Return
	EndIf
	TailActor = marker.PlaceActorAtMe(ActorBase_Tail)
	If TailActor == None
		Return
	EndIf
	CurrentTailMarker = marker
	TailOutcomeProcessed = False
	Alias_Tail.ForceRefTo(TailActor)
	RegisterForRemoteEvent(TailActor, "OnDying")
	WaitFor3D(TailActor)
	If TailActor == None
		Return
	EndIf
	RegisterForAnimationEvent(TailActor, TailAttackHitEvent)
	RegisterForAnimationEvent(TailActor, AnimDoneEvent)
	TailState = TailStateAscending
	TailActor.PlayIdle(TailAscendIdle)
	StartTimer(6.0, 6)
EndFunction

Function BeginTailAttack()
	If TailActor == None || TailActor.IsDead() || TailState != TailStateIdle || Phase >= PhaseOutro
		Return
	EndIf
	TailState = TailStateAttacking
	TailActor.PlayIdle(TailAttackIdle)
	StartTimer(8.0, 6)
EndFunction

Function BeginTailDescend()
	TailState = TailStateDescending
	TailActor.PlayIdle(TailDescendIdle)
	StartTimer(6.0, 6)
EndFunction

Function OnTailAnimationDone()
	CancelTimer(6)
	If TailActor == None || TailActor.IsDead()
		Return
	EndIf
	If TailState == TailStateAscending
		TailState = TailStateIdle
		StartTimer(TailAttackTime.GetValue(), TailAttackTimerID)
	ElseIf TailState == TailStateAttacking
		If !TailOutcomeProcessed
			SmashTailQuarter()
		EndIf
		BeginTailDescend()
	ElseIf TailState == TailStateDescending
		DespawnTail()
		ArmTailSpawn()
	EndIf
EndFunction

Function DespawnTail()
	CancelTimer(TailAttackTimerID)
	CancelTimer(6)
	CancelTimer(10)
	Actor tail = TailActor
	TailActor = None
	CurrentTailMarker = None
	Alias_Tail.Clear()
	If tail != None
		UnregisterForRemoteEvent(tail, "OnDying")
		UnregisterForAnimationEvent(tail, TailAttackHitEvent)
		UnregisterForAnimationEvent(tail, AnimDoneEvent)
		tail.Disable()
		tail.Delete()
	EndIf
EndFunction

Function SmashTailQuarter()
	TailOutcomeProcessed = True
	If CurrentTailMarker == None
		Return
	EndIf
	Raids:RD01:Enc06:PlatformDestructionScript platform = PlatformForTailMarker(CurrentTailMarker)
	If !IsQuarterIntact(platform)
		Return
	EndIf
	If IsLastIslandQuarterProtected() && CountIntactQuarters() <= 1
		Return
	EndIf
	platform.CallFunctionNoWait("AnimateDestroy", new Var[0])
	; The tail marker's default link is the stage-slice enable marker parenting the lava pillars
	; that stand on this quarter.
	ObjectReference slice = CurrentTailMarker.GetLinkedRef()
	If slice != None
		slice.Disable()
	EndIf
EndFunction

; Solo rule: Tales sets GLOB B21_TFA_glob_RaidProtectLastIslandQuarter (Tales FFF1E0) from its
; [Raids] ini. Without Tales, or with the global at 0, the tail can destroy every quarter (FO76).
Bool Function IsLastIslandQuarterProtected()
	If !Game.IsPluginInstalled("B21_TalesFromAppalachia.esm")
		Return False
	EndIf
	GlobalVariable protectLastQuarter = Game.GetFormFromFile(0x00FFF1E0, "B21_TalesFromAppalachia.esm") as GlobalVariable
	If protectLastQuarter == None
		Return False
	EndIf
	Return protectLastQuarter.GetValue() > 0.0
EndFunction

Raids:RD01:Enc06:PlatformDestructionScript Function PlatformForTailMarker(ObjectReference akMarker)
	If akMarker == None
		Return None
	EndIf
	Return akMarker.GetLinkedRef(LinkCustom01) as Raids:RD01:Enc06:PlatformDestructionScript
EndFunction

; Destroyed is only set after the destruction animation ends, so the state name covers a quarter
; that is still breaking apart.
Bool Function IsQuarterIntact(Raids:RD01:Enc06:PlatformDestructionScript akPlatform)
	If akPlatform == None
		Return False
	EndIf
	Return !akPlatform.IsPlatformDestroyed() && akPlatform.GetState() != "Destroyed"
EndFunction

Int Function CountIntactQuarters()
	If TailMarkers == None
		Return 0
	EndIf
	Int intact = 0
	Int i = 0
	While i < TailMarkers.Length
		If IsQuarterIntact(PlatformForTailMarker(TailMarkers[i]))
			intact += 1
		EndIf
		i += 1
	EndWhile
	Return intact
EndFunction

ObjectReference Function PickTailMarker()
	Int intact = CountIntactQuarters()
	If intact == 0
		Return None
	EndIf
	Int pick = Utility.RandomInt(0, intact - 1)
	Int i = 0
	While i < TailMarkers.Length
		If IsQuarterIntact(PlatformForTailMarker(TailMarkers[i]))
			If pick == 0
				Return TailMarkers[i]
			EndIf
			pick -= 1
		EndIf
		i += 1
	EndWhile
	Return None
EndFunction

; ---- Phase 3 lava pillars ----

Function EruptLavaPillars(Int aiCount)
	Int total = Alias_LavaPillarMarkers.GetCount()
	If total <= 0
		Return
	EndIf
	Int erupted = 0
	Int attempts = 0
	While erupted < aiCount && attempts < total * 2
		ObjectReference marker = Alias_LavaPillarMarkers.GetAt(Utility.RandomInt(0, total - 1))
		If marker != None && !marker.IsDisabled()
			marker.PlaceAtMe(LavaPillarExplosion)
			erupted += 1
		EndIf
		attempts += 1
	EndWhile
EndFunction

; ---- Outro, success, failure ----

Function BeginOutro()
	If OutroProcessed || IsStageDone(9000) || HeadActor == None || HeadActor.IsDead()
		Return
	EndIf
	OutroProcessed = True
	Phase = PhaseOutro
	CancelTimer(HeadRelocationTimerID)
	CancelTimer(TailSpawnTimerID)
	CancelTimer(5)
	CancelTimer(8)
	DespawnTail()
	HeadState = HeadStateIdle
	HeadActor.SetGhost(False)
	HeadActor.AddKeyword(RD01_Enc06_IgnoreCombat_Keyword)
	HeadActor.AddPerk(OutroDamageMultPerk)
	EnsureHeadAnimationEvents()
	HeadActor.PlayIdle(ChargeIdle)
	StartTimer(ChargeMessageWaitLength, ChargeMessageTimerID)
	; The charge-up clip raises Play03 at 26.9 s; this fires the blast if that event is lost.
	StartTimer(30.0, 11)
EndFunction

Function OutroBlast()
	If Phase != PhaseOutro || HeadActor == None || HeadActor.IsDead() || IsStageDone(9000)
		Return
	EndIf
	CancelTimer(11)
	Phase = PhaseFailure
	HeadActor.PlaceAtMe(OutroAttackExplosion)
	PlayPhaseTrack()
	StartTimer(OutroLength, 4)
EndFunction

; The converted blast only knocks down, so a player who was not thrown into the lava still loses.
Function FailOutro()
	If Phase != PhaseFailure || IsStageDone(9000)
		Return
	EndIf
	Actor player = Game.GetPlayer()
	If !player.IsDead()
		player.Kill(HeadActor)
	EndIf
EndFunction

; Stage 9000 is set by DefaultAliasOnDeath on the head. The raid controller owns the master
; owner stage and stage 10000 (rewards), so this module never sets them.
Function HandleEncounterSuccess()
	If Phase == PhaseSuccess
		Return
	EndIf
	Phase = PhaseSuccess
	CancelAllTimers()
	DespawnTail()
	If HeadActor != None
		HeadActor.SetGhost(False)
		HeadActor.RemoveKeyword(RD01_Enc06_IgnoreCombat_Keyword)
		HeadActor.RemovePerk(OutroDamageMultPerk)
	EndIf
	PlayPhaseTrack()
	LowerBody()
	Raids:RD01:Enc06:LavafallVFXScript lavafall = Alias_LavaFallVFXMarker.GetReference() as Raids:RD01:Enc06:LavafallVFXScript
	If lavafall != None
		lavafall.ClientPlayVFX()
	EndIf
	ObjectReference walkway = Alias_WalkwayEnableMarker.GetReference()
	If walkway != None
		walkway.Enable()
	EndIf
EndFunction
