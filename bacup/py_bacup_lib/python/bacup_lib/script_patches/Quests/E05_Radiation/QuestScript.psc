Event OnQuestInit()
	; FO4 keeps script variables across Stop/Start, while FO76 created a fresh quest instance per run.
	B21Scavengers = New Actor[0]
	B21OreVeins = New ObjectReference[0]
	B21HarvestedOreVeins = New ObjectReference[0]
	BossWavesPicked = New Int[0]
	BossWaveType = -1
	SpawnTunnel01 = -1
	SpawnTunnel02 = -1
	TunnelEncWaveRange01 = -1
	TunnelEncWaveRange02 = -1
	ScavengersTotal = 0
	ScavengersAlive = 0
	Quest owner = Self as Quest
	EWSScript = owner as DefaultQuestEncounterWaveScript
	eventQuestScript = owner as DefaultEventQuest
	PublishOreProgress()
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID != 5628
		Return
	EndIf
	If !IsRunning() || IsOperationOver()
		Return
	EndIf
	RespawnHarvestedOreVeins()
	StartOreRespawnTimer()
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
	UnregisterForRemoteEvent(akSender, "OnDeath")
	If B21Scavengers == None || B21Scavengers.Find(akSender) < 0
		Return
	EndIf
	RefreshScavengerCount()
	If !IsRunning() || IsOperationOver()
		Return
	EndIf
	If ScavengersAlive > 0
		ShowOperationMessage(E05_Radiation_ScavengerDeadMessage)
		If E05_Radiation_ScavengerDeadScene != None && !E05_Radiation_ScavengerDeadScene.IsPlaying()
			E05_Radiation_ScavengerDeadScene.Start()
		EndIf
	Else
		ShowOperationMessage(E05_Radiation_AllScavengersDeadMessage)
		If !IsStageDone(800)
			SetStage(800)
		EndIf
	EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	If B21OreVeins == None || B21OreVeins.Find(akSender) < 0 || !(akActionRef is Actor)
		Return
	EndIf
	; Harvesting the flora hands out the ore; the vein stays spent until the respawn tick replaces it.
	UnregisterForRemoteEvent(akSender, "OnActivate")
	If OreVein_RefColl != None
		OreVein_RefColl.RemoveRef(akSender)
	EndIf
	If B21HarvestedOreVeins != None && B21HarvestedOreVeins.Find(akSender) < 0
		B21HarvestedOreVeins.Add(akSender)
	EndIf
EndEvent

Bool Function IsOperationOver()
	Return IsStageDone(750) || IsStageDone(800) || IsStageDone(900) || IsStageDone(9000) || IsStageDone(9991) || IsStageDone(9992) || IsStageDone(9993)
EndFunction

Function EnsureOperationState()
	If B21Scavengers == None
		B21Scavengers = New Actor[0]
	EndIf
	If B21OreVeins == None
		B21OreVeins = New ObjectReference[0]
	EndIf
	If B21HarvestedOreVeins == None
		B21HarvestedOreVeins = New ObjectReference[0]
	EndIf
	If BossWavesPicked == None
		BossWavesPicked = New Int[0]
	EndIf
	If EWSScript == None
		Quest owner = Self as Quest
		EWSScript = owner as DefaultQuestEncounterWaveScript
	EndIf
EndFunction

Function PublishVariable(String asName, Float afValue)
	Quest owner = Self as Quest
	B21:QuestVariables variables = owner as B21:QuestVariables
	If variables != None
		variables.SetVariable(asName, afValue)
	EndIf
EndFunction

Function ShowOperationMessage(Message akMessage)
	If akMessage != None
		akMessage.Show()
	EndIf
EndFunction

Function SetReferencesEnabled(ObjectReference[] akRefs, Bool abEnabled)
	Int index = 0
	While akRefs != None && index < akRefs.Length
		ObjectReference target = akRefs[index]
		If target != None
			If abEnabled
				target.Enable(False)
			Else
				target.Disable(False)
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

Function SetOreLightLit(Int aiIndex, Bool abLit)
	; The lit mesh and its red light start disabled beside the enabled unlit mesh.
	If OreLightsOn != None && aiIndex < OreLightsOn.Length && OreLightsOn[aiIndex] != None
		If abLit
			OreLightsOn[aiIndex].Enable(False)
		Else
			OreLightsOn[aiIndex].Disable(False)
		EndIf
	EndIf
	If OreRedLights != None && aiIndex < OreRedLights.Length && OreRedLights[aiIndex] != None
		If abLit
			OreRedLights[aiIndex].Enable(False)
		Else
			OreRedLights[aiIndex].Disable(False)
		EndIf
	EndIf
	If OreLightsOff != None && aiIndex < OreLightsOff.Length && OreLightsOff[aiIndex] != None
		If abLit
			OreLightsOff[aiIndex].Disable(False)
		Else
			OreLightsOff[aiIndex].Enable(False)
		EndIf
	EndIf
EndFunction

ReferenceAlias Function ScavengerAlias(Int aiIndex)
	If aiIndex == 0
		Return Scavenger01
	ElseIf aiIndex == 1
		Return Scavenger02
	ElseIf aiIndex == 2
		Return Scavenger03
	ElseIf aiIndex == 3
		Return Scavenger04
	EndIf
	Return None
EndFunction

Function SpawnScavengers()
	EnsureOperationState()
	If B21Scavengers.Length > 0 || E05_Radiation_Scavenger_Hazmat == None || ScavengerSpawnPointXMarkers == None
		Return
	EndIf
	Int index = 0
	While index < ScavengerSpawnPointXMarkers.Length && index < 4
		ObjectReference spawnMarker = ScavengerSpawnPointXMarkers[index]
		If spawnMarker != None
			Actor scavenger = spawnMarker.PlaceActorAtMe(E05_Radiation_Scavenger_Hazmat)
			If scavenger != None
				B21Scavengers.Add(scavenger)
				ReferenceAlias scavengerSlot = ScavengerAlias(index)
				If scavengerSlot != None
					scavengerSlot.ForceRefTo(scavenger)
				EndIf
				If Scavengers != None
					Scavengers.AddRef(scavenger)
				EndIf
				RegisterForRemoteEvent(scavenger, "OnDeath")
			EndIf
		EndIf
		index += 1
	EndWhile
	ScavengersTotal = B21Scavengers.Length
	RefreshScavengerCount()
EndFunction

Function RefreshScavengerCount()
	Int alive = 0
	Int index = 0
	While B21Scavengers != None && index < B21Scavengers.Length
		Actor scavenger = B21Scavengers[index]
		If scavenger != None && !scavenger.IsDead()
			alive += 1
		EndIf
		index += 1
	EndWhile
	ScavengersAlive = alive
	PublishVariable("ScavengersCount", alive as Float)
	PublishVariable("TotalScavengers", ScavengersTotal as Float)
EndFunction

Int Function CountLivingScavengers()
	RefreshScavengerCount()
	Return ScavengersAlive
EndFunction

Function SpawnOreVeins()
	EnsureOperationState()
	If B21OreVeins.Length > 0 || E05_Radiation_VeinOre == None || OreSpawnLocs == None
		Return
	EndIf
	Int count = OreSpawnLocs.GetCount()
	Int index = 0
	; Papyrus arrays hold at most 128 entries.
	While index < count && B21OreVeins.Length < 120
		ObjectReference spawnMarker = OreSpawnLocs.GetAt(index)
		If spawnMarker != None
			PlaceOreVein(spawnMarker)
		EndIf
		index += 1
	EndWhile
EndFunction

ObjectReference Function PlaceOreVein(ObjectReference akAnchor)
	ObjectReference vein = akAnchor.PlaceAtMe(E05_Radiation_VeinOre, 1, False, False, True)
	If vein == None
		Return None
	EndIf
	B21OreVeins.Add(vein)
	If OreVein_RefColl != None
		OreVein_RefColl.AddRef(vein)
	EndIf
	RegisterForRemoteEvent(vein, "OnActivate")
	Return vein
EndFunction

Function StartOreRespawnTimer()
	If OreRespawnTimerStage >= 0 && !IsStageDone(OreRespawnTimerStage)
		Return
	EndIf
	Float delay = OreRespawnTimer
	If delay < 1.0
		delay = 1.0
	EndIf
	StartTimer(delay, 5628)
EndFunction

Function RespawnHarvestedOreVeins()
	EnsureOperationState()
	Int budget = NumOresToRespawn
	If budget < 1
		budget = 1
	EndIf
	While budget > 0 && B21HarvestedOreVeins.Length > 0
		ObjectReference spentVein = B21HarvestedOreVeins[0]
		B21HarvestedOreVeins.Remove(0)
		Int veinIndex = B21OreVeins.Find(spentVein)
		If veinIndex >= 0
			B21OreVeins.Remove(veinIndex)
		EndIf
		If spentVein != None
			PlaceOreVein(spentVein)
			spentVein.Disable(False)
			spentVein.Delete()
		EndIf
		budget -= 1
	EndWhile
EndFunction

Float Function GetOreProgress()
	Quest owner = Self as Quest
	Quests:_Default:ProgressBar:MasterScript progressBar = owner as Quests:_Default:ProgressBar:MasterScript
	If progressBar == None
		Return 0.0
	EndIf
	Return progressBar.GetProgress()
EndFunction

Int Function GetOreRewardLevel()
	Int rewardLevel = 0
	If IsStageDone(400)
		rewardLevel = 1
	EndIf
	If IsStageDone(500)
		rewardLevel = 2
	EndIf
	If IsStageDone(600)
		rewardLevel = 3
	EndIf
	If IsStageDone(700)
		rewardLevel = 4
	EndIf
	Return rewardLevel
EndFunction

Function PublishOreProgress()
	PublishVariable("OreCollected", GetOreProgress() * ProgressMult)
	PublishVariable("OreRewardLvl", GetOreRewardLevel() as Float)
	PublishVariable("MaxOreRewardLvl", 4.0)
	PublishVariable("OreGoal1", OreGoal1 as Float)
	PublishVariable("OreGoal2", OreGoal2 as Float)
	PublishVariable("OreGoal3", OreGoal3 as Float)
EndFunction

Function SetGoalStageIfReached(Float afProgress, Int aiGoal, Int aiStage)
	If aiGoal > 0 && afProgress >= aiGoal as Float && !IsStageDone(aiStage)
		SetStage(aiStage)
	EndIf
EndFunction

Function HandleOreDeposited(Int aiCount)
	If aiCount <= 0 || !IsRunning() || !IsStageDone(300) || IsOperationOver()
		Return
	EndIf
	; OreGoal values are progress-bar percentages: ModPercentage 2 per ore fills 100 at 50 ore.
	Float progress = GetOreProgress()
	SetGoalStageIfReached(progress, OreGoal1, 400)
	SetGoalStageIfReached(progress, OreGoal2, 500)
	SetGoalStageIfReached(progress, OreGoal3, 600)
	SetGoalStageIfReached(progress, OreGoal4, 700)
	PublishOreProgress()
EndFunction

Function StartOperationWaves()
	EnsureOperationState()
	If EWSScript == None
		Return
	EndIf
	EWSScript.StartEncounterWaveByID("Rad Enemies 01")
	EWSScript.StartEncounterWaveByID("Rad Enemies 02")
	EWSScript.StartEncounterWaveByID("Rad Enemies 03")
	EWSScript.StartEncounterWaveByID("Rad Enemies 04")
	EWSScript.StartEncounterWaveByID("Rad Enemies Tunnel 04")
EndFunction

String Function BossWaveFamily(Int aiFamily)
	If aiFamily == 0
		Return "Floaters"
	ElseIf aiFamily == 1
		Return "RadCrickets"
	ElseIf aiFamily == 2
		Return "RadSnallygasters"
	EndIf
	Return "RadDeathclaws"
EndFunction

Function SpawnGoalBossWave()
	EnsureOperationState()
	If EWSScript == None
		Return
	EndIf
	; Each ore tier sends one creature family, preferring families not used yet this run, out of two different tunnels.
	Int family = Utility.RandomInt(0, 3)
	Int attempts = 0
	While attempts < 4 && BossWavesPicked.Find(family) >= 0
		family = (family + 1) % 4
		attempts += 1
	EndWhile
	If BossWavesPicked.Find(family) < 0
		BossWavesPicked.Add(family)
	EndIf
	BossWaveType = family
	SpawnTunnel01 = Utility.RandomInt(0, 3)
	SpawnTunnel02 = (SpawnTunnel01 + Utility.RandomInt(1, 3)) % 4
	String familyName = BossWaveFamily(family)
	TunnelEncWaveRange01 = EWSScript.FindEncounterWaveIndex(familyName + " Tunnel 0" + (SpawnTunnel01 + 1))
	TunnelEncWaveRange02 = EWSScript.FindEncounterWaveIndex(familyName + " Tunnel 0" + (SpawnTunnel02 + 1))
	EWSScript.StartEncounterWave(TunnelEncWaveRange01)
	EWSScript.StartEncounterWave(TunnelEncWaveRange02)
EndFunction

Function BeginOperation()
	EnsureOperationState()
	If IsOperationOver()
		Return
	EndIf
	SpawnScavengers()
	SetReferencesEnabled(AmmoBoxes, True)
	SpawnOreVeins()
	StartOperationWaves()
	RefreshScavengerCount()
	PublishOreProgress()
	CancelTimer(5628)
	StartOreRespawnTimer()
EndFunction

Function HandleOreGoalReached(Int aiGoal)
	If aiGoal < 1 || aiGoal > 4
		Return
	EndIf
	SetOreLightLit(aiGoal - 1, True)
	PublishOreProgress()
	If IsRunning() && !IsOperationOver()
		SpawnGoalBossWave()
	EndIf
EndFunction

Function StopOperationWaves()
	EnsureOperationState()
	If EWSScript != None
		EWSScript.StopAllEncounterWaves(False)
	EndIf
EndFunction

Function CleanupOperation()
	CancelTimer(5628)
	Int index = 0
	While B21OreVeins != None && index < B21OreVeins.Length
		ObjectReference vein = B21OreVeins[index]
		If vein != None
			UnregisterForRemoteEvent(vein, "OnActivate")
			vein.Disable(False)
			vein.Delete()
		EndIf
		index += 1
	EndWhile
	index = 0
	While B21HarvestedOreVeins != None && index < B21HarvestedOreVeins.Length
		ObjectReference spentVein = B21HarvestedOreVeins[index]
		If spentVein != None
			spentVein.Disable(False)
			spentVein.Delete()
		EndIf
		index += 1
	EndWhile
	index = 0
	While B21Scavengers != None && index < B21Scavengers.Length
		Actor scavenger = B21Scavengers[index]
		If scavenger != None
			UnregisterForRemoteEvent(scavenger, "OnDeath")
			If !scavenger.IsDead()
				scavenger.DisableNoWait()
			EndIf
			scavenger.Delete()
		EndIf
		index += 1
	EndWhile
	SetReferencesEnabled(AmmoBoxes, False)
	index = 0
	While index < 4
		SetOreLightLit(index, False)
		index += 1
	EndWhile
	B21OreVeins = New ObjectReference[0]
	B21HarvestedOreVeins = New ObjectReference[0]
	B21Scavengers = New Actor[0]
	ScavengersAlive = 0
EndFunction
