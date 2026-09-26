; Activity: Riding Shotgun (560B13). The converted records already carry the whole skeleton of the
; run: the six scenes set stages from their phase completions (SCQS) and wait on stages in their
; phase conditions, the checkpoint markers set arrival stages through DefaultAliasOnDistanceLessThan
; (re-registered on the LEADER brahmin alias), the raider collections close each fight through
; DefaultCollectionAliasOnDeath, the guard alias hands out the start stage on activation, the supply
; refs report pickups, and the converter attached B21:QuestTimer (1500 s from stage 100),
; B21:ObjectiveTimers (objective 10, 180 s, expiry 9990), B21:QuestVariables (SupplyCount),
; B21:EncounterWaveCatalog and B21:QuestRewards (stage 9000). These fragments supply what FO76 ran
; server side: role assignment, marker hand-off between checkpoints, wave starts and stops, scene
; starts, announcements, obstacle handling, objectives and the success/failure/cleanup chains.

Bool Function IsCaravanRunOver()
	Return IsStageDone(9000) || IsStageDone(9990)
EndFunction

Function CompleteOpenObjective(Int aiObjective)
	If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
		SetObjectiveCompleted(aiObjective, True)
	EndIf
EndFunction

Function FailOpenObjective(Int aiObjective)
	If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
		SetObjectiveFailed(aiObjective, True)
	EndIf
EndFunction

Function CompleteOpenCaravanObjectives()
	CompleteOpenObjective(10)
	CompleteOpenObjective(50)
	CompleteOpenObjective(100)
	CompleteOpenObjective(150)
	CompleteOpenObjective(175)
	CompleteOpenObjective(200)
	CompleteOpenObjective(300)
	CompleteOpenObjective(400)
	CompleteOpenObjective(450)
	CompleteOpenObjective(500)
	CompleteOpenObjective(900)
EndFunction

Function FailOpenCaravanObjectives()
	FailOpenObjective(10)
	FailOpenObjective(50)
	FailOpenObjective(100)
	FailOpenObjective(150)
	FailOpenObjective(175)
	FailOpenObjective(200)
	FailOpenObjective(300)
	FailOpenObjective(400)
	FailOpenObjective(450)
	FailOpenObjective(500)
	FailOpenObjective(900)
	FailOpenObjective(2001)
	FailOpenObjective(2002)
EndFunction

Function SetCaravanStage(Int aiStage)
	If !IsStageDone(aiStage)
		SetStage(aiStage)
	EndIf
EndFunction

Function ShowCaravanMessage(Message akMessage)
	If akMessage != None
		akMessage.Show()
	EndIf
EndFunction

; Stage fragments that were already queued must not restart the run after it ended.
Function StartCaravanScene(Scene akScene)
	If akScene != None && !akScene.IsPlaying() && !IsCaravanRunOver()
		akScene.Start()
	EndIf
EndFunction

Function StopCaravanScene(Scene akScene)
	If akScene != None && akScene.IsPlaying()
		akScene.Stop()
	EndIf
EndFunction

Function StartCaravanWave(String asWaveID)
	If IsCaravanRunOver()
		Return
	EndIf

	Quest owner = Self as Quest
	DefaultQuestEncounterWaveScript waveScript = owner as DefaultQuestEncounterWaveScript
	If waveScript != None
		waveScript.StartEncounterWaveByID(asWaveID)
	EndIf
EndFunction

Function StopCaravanWave(String asWaveID)
	Quest owner = Self as Quest
	DefaultQuestEncounterWaveScript waveScript = owner as DefaultQuestEncounterWaveScript
	If waveScript != None
		waveScript.StopEncounterWaveByID(asWaveID, False)
	EndIf
EndFunction

Function StopAllCaravanWaves()
	Quest owner = Self as Quest
	DefaultQuestEncounterWaveScript waveScript = owner as DefaultQuestEncounterWaveScript
	If waveScript != None
		waveScript.StopAllEncounterWaves(False)
	EndIf
EndFunction

Function StopCheckpoint01Waves()
	StopCaravanWave("Checkpoint01_LeftSniper")
	StopCaravanWave("Checkpoint01_RightSnipers")
	StopCaravanWave("Checkpoint01_ShortRange")
	StopCaravanWave("Checkpoint01_Flank")
EndFunction

Function StopCheckpoint03Waves()
	StopCaravanWave("Checkpoint03_Back")
	StopCaravanWave("Checkpoint03_Mid")
	StopCaravanWave("Checkpoint03_Building")
	StopCaravanWave("Checkpoint03_BossMinions")
	StopCaravanWave("Checkpoint 03 - CaptainBoss")
EndFunction

Function PointCaravanMarker(ReferenceAlias akCurrentAlias, ReferenceAlias akCheckpointAlias)
	If akCurrentAlias == None || akCheckpointAlias == None
		Return
	EndIf

	ObjectReference checkpointRef = akCheckpointAlias.GetReference()
	If checkpointRef != None
		akCurrentAlias.ForceRefTo(checkpointRef)
	EndIf
EndFunction

; The caravan's travel packages all target the *_CP_CURRENT aliases, so moving the convoy to the
; next checkpoint means re-pointing those four markers.
Function AdvanceCaravanTo(ReferenceAlias akBrahmin1Marker, ReferenceAlias akBrahmin2Marker, ReferenceAlias akGuardMarker, ReferenceAlias akMerchantMarker)
	PointCaravanMarker(alias_B1_CP_CURRENT, akBrahmin1Marker)
	PointCaravanMarker(alias_B2_CP_CURRENT, akBrahmin2Marker)
	PointCaravanMarker(alias_G_CP_CURRENT, akGuardMarker)
	PointCaravanMarker(alias_M_CP_CURRENT, akMerchantMarker)
EndFunction

; Nothing in the converted data reads E05_Caravan_Global_BrahminStop any more; the halted state is
; mirrored into it so the convoy's stop/go is still observable where FO76 exposed it.
Function SetCaravanHalted(Bool abHalted)
	If global_BrahminStop == None
		Return
	EndIf

	If abHalted
		global_BrahminStop.SetValue(1.0)
	Else
		global_BrahminStop.SetValue(0.0)
	EndIf
EndFunction

Function SetCaravanRefEnabled(ReferenceAlias akAlias, Bool abEnabled)
	If akAlias == None
		Return
	EndIf

	ObjectReference aliasRef = akAlias.GetReference()
	If aliasRef == None
		Return
	EndIf

	If abEnabled
		aliasRef.Enable(False)
	Else
		aliasRef.Disable(False)
	EndIf
EndFunction

Function DisableAliasByID(Int aiAliasID)
	ReferenceAlias targetAlias = GetAlias(aiAliasID) as ReferenceAlias
	SetCaravanRefEnabled(targetAlias, False)
EndFunction

Int Function CountLivingCollection(RefCollectionAlias akCollection)
	If akCollection == None
		Return 0
	EndIf

	Int living = 0
	Int index = 0
	While index < akCollection.GetCount()
		Actor member = akCollection.GetAt(index) as Actor
		If member != None && !member.IsDead()
			living += 1
		EndIf
		index += 1
	EndWhile
	Return living
EndFunction

Function SetCaravanBrahminGhost(Bool abGhost)
	If alias_Brahmin01 != None
		Actor frontBrahmin = alias_Brahmin01.GetActorReference()
		If frontBrahmin != None
			frontBrahmin.SetGhost(abGhost)
		EndIf
	EndIf
	If alias_Brahmin02 != None
		Actor backBrahmin = alias_Brahmin02.GetActorReference()
		If backBrahmin != None
			backBrahmin.SetGhost(abGhost)
		EndIf
	EndIf
EndFunction

; FO76 handed each player their own lost supplies through an item dispenser. In one-player FO4 a
; single persistent copy is placed on each crate and forced into the matching quest-item alias, so
; DefaultAliasOnContainerChangedTo still reports the pickup on stages 1000-1003.
Function PlaceLostSupply(ReferenceAlias akCrateAlias, ReferenceAlias akSupplyAlias)
	If akCrateAlias == None || akSupplyAlias == None || pE05_Caravan_Supplies == None
		Return
	EndIf
	If akSupplyAlias.GetReference() != None
		Return
	EndIf

	ObjectReference crateRef = akCrateAlias.GetReference()
	If crateRef == None
		Return
	EndIf

	crateRef.Enable(False)
	ObjectReference supplyRef = crateRef.PlaceAtMe(pE05_Caravan_Supplies, 1, True, False, False)
	If supplyRef != None
		akSupplyAlias.ForceRefTo(supplyRef)
	EndIf
EndFunction

Function PlaceLostSupplies()
	PlaceLostSupply(alias_R_Crate01, Alias_ref_supplies_01)
	PlaceLostSupply(alias_R_Crate02, Alias_ref_supplies_02)
	PlaceLostSupply(alias_R_Crate03, Alias_ref_supplies_03)
	PlaceLostSupply(alias_R_Crate04, Alias_ref_supplies_04)
EndFunction

Function RemoveLostSupply(ReferenceAlias akSupplyAlias)
	If akSupplyAlias == None
		Return
	EndIf

	ObjectReference supplyRef = akSupplyAlias.GetReference()
	If supplyRef != None && supplyRef.GetContainer() == None
		supplyRef.Delete()
	EndIf
	akSupplyAlias.Clear()
EndFunction

Function RemoveLostSupplies()
	RemoveLostSupply(Alias_ref_supplies_01)
	RemoveLostSupply(Alias_ref_supplies_02)
	RemoveLostSupply(Alias_ref_supplies_03)
	RemoveLostSupply(Alias_ref_supplies_04)
	SetCaravanRefEnabled(alias_R_Crate01, False)
	SetCaravanRefEnabled(alias_R_Crate02, False)
	SetCaravanRefEnabled(alias_R_Crate03, False)
	SetCaravanRefEnabled(alias_R_Crate04, False)
	SetCaravanRefEnabled(alias_R_Crate05, False)
EndFunction

Function RefreshCollectedSupplyCount()
	Int collected = 0
	If IsStageDone(1000)
		collected += 1
	EndIf
	If IsStageDone(1001)
		collected += 1
	EndIf
	If IsStageDone(1002)
		collected += 1
	EndIf
	If IsStageDone(1003)
		collected += 1
	EndIf

	Quest owner = Self as Quest
	B21:QuestVariables questVariables = owner as B21:QuestVariables
	If questVariables != None
		questVariables.SetVariable("SupplyCount", collected as Float)
	EndIf

	If !IsObjectiveDisplayed(9000)
		SetObjectiveDisplayed(9000, True)
	EndIf
	If collected >= 4
		CompleteOpenObjective(9000)
	EndIf
EndFunction

Function CheckCaravanStartPositions()
	If IsStageDone(15) && IsStageDone(20) && IsStageDone(25)
		SetCaravanStage(30)
	EndIf
EndFunction

Function CheckCheckpoint03Objectives()
	If IsStageDone(425) && IsStageDone(450)
		SetCaravanStage(460)
	EndIf
EndFunction

Function CleanUpCaravan()
	StopAllCaravanWaves()
	StopCaravanScene(scene_SetUp)
	StopCaravanScene(scene_E05_Caravan_VinnyAnnounce)
	StopCaravanScene(scene_TraveltoCP01)
	StopCaravanScene(scene_TraveltoCP02)
	StopCaravanScene(scene_TraveltoCP03)
	StopCaravanScene(scene_TraveltoCP04)
	StopCaravanScene(scene_TraveltoCP05)

	If alias_door_Exit != None && alias_door_Exit.GetReference() != None
		alias_door_Exit.GetReference().BlockActivation(False)
	EndIf

	SetCaravanBrahminGhost(False)
	RemoveLostSupplies()
	SetCaravanHalted(False)

	SetCaravanRefEnabled(alias_R_Wall01, False)
	SetCaravanRefEnabled(alias_R_Wall01_02, False)
	SetCaravanRefEnabled(alias_R_Wall02, False)
	SetCaravanRefEnabled(alias_R_Wall04, False)
	SetCaravanRefEnabled(alias_R_Wall04_02, False)
	SetCaravanRefEnabled(alias_R_Barricade02, False)
	SetCaravanRefEnabled(alias_R_Bomb02, False)

	Quest owner = Self as Quest
	If (owner as Quests:E05_Caravan:QuestScript) != None
		(owner as Quests:E05_Caravan:QuestScript).SetCaravanObstaclesEnabled(False)
		(owner as Quests:E05_Caravan:QuestScript).SetCaravanCratesEnabled(False)
	EndIf

	If alias_B1_CP_CURRENT != None
		alias_B1_CP_CURRENT.Clear()
	EndIf
	If alias_B2_CP_CURRENT != None
		alias_B2_CP_CURRENT.Clear()
	EndIf
	If alias_G_CP_CURRENT != None
		alias_G_CP_CURRENT.Clear()
	EndIf
	If alias_M_CP_CURRENT != None
		alias_M_CP_CURRENT.Clear()
	EndIf

	; The dialogue quest's Master_QuestScript sees the closed gate and starts the 20 minute cooldown.
	If global_IsRunning != None
		global_IsRunning.SetValue(0.0)
	EndIf
EndFunction

Function Fragment_Stage_0004_Item_00()
	If global_IsRunning != None
		global_IsRunning.SetValue(0.0)
	EndIf

	If alias_BrahminLeader != None && alias_Brahmin01 != None && alias_Brahmin01.GetReference() != None
		alias_BrahminLeader.ForceRefTo(alias_Brahmin01.GetReference())
	EndIf
	If alias_BrahminFollower != None && alias_Brahmin02 != None && alias_Brahmin02.GetReference() != None
		alias_BrahminFollower.ForceRefTo(alias_Brahmin02.GetReference())
	EndIf

	AdvanceCaravanTo(alias_B1_StartPos, alias_B2_StartPos, alias_G_StartPos, alias_M_StartPos)
	SetCaravanHalted(False)

	Quest owner = Self as Quest
	If (owner as Quests:E05_Caravan:QuestScript) != None
		(owner as Quests:E05_Caravan:QuestScript).SetCaravanObstaclesEnabled(True)
		(owner as Quests:E05_Caravan:QuestScript).AssignCaravanRoles()
	EndIf

	SetCaravanRefEnabled(alias_R_Bomb02, False)
	SetObjectiveDisplayed(10, True, True)
EndFunction

Function Fragment_Stage_0005_Item_00()
	SetCaravanStage(10)
EndFunction

Function Fragment_Stage_0010_Item_00()
	StartCaravanScene(scene_SetUp)
	StartCaravanScene(scene_E05_Caravan_VinnyAnnounce)
EndFunction

Function Fragment_Stage_0011_Item_00()
	DisableAliasByID(241)
EndFunction

Function Fragment_Stage_0012_Item_00()
	DisableAliasByID(242)
EndFunction

Function Fragment_Stage_0013_Item_00()
	DisableAliasByID(244)
	DisableAliasByID(245)
EndFunction

Function Fragment_Stage_0015_Item_00()
	CheckCaravanStartPositions()
EndFunction

Function Fragment_Stage_0020_Item_00()
	CheckCaravanStartPositions()
EndFunction

Function Fragment_Stage_0025_Item_00()
	CheckCaravanStartPositions()
EndFunction

Function Fragment_Stage_0075_Item_00()
	CompleteOpenObjective(10)
	SetObjectiveDisplayed(50, True, True)
	StopCaravanScene(scene_SetUp)
	StartCaravanScene(scene_TraveltoCP01)
	PlaceLostSupplies()
	RefreshCollectedSupplyCount()
EndFunction

Function Fragment_Stage_0076_Item_00()
	SetCaravanStage(11)
	SetCaravanStage(12)
EndFunction

Function Fragment_Stage_0100_Item_00()
	CompleteOpenObjective(50)
	SetObjectiveDisplayed(100, True, True)
	SetObjectiveDisplayed(150, True)
	AdvanceCaravanTo(alias_B1_CP01, alias_B2_CP01, alias_G_CP01, alias_M_CP01)
	SetCaravanHalted(False)
EndFunction

Function Fragment_Stage_0201_Item_00()
	SetCaravanHalted(True)
	StartCaravanWave("Checkpoint01_LeftSniper")
	StartCaravanWave("Checkpoint01_RightSnipers")
	StartCaravanWave("Checkpoint01_ShortRange")
	SetObjectiveDisplayed(200, True, True)
	SetCaravanStage(202)
EndFunction

Function Fragment_Stage_0202_Item_00()
	ShowCaravanMessage(E05_Caravan_Message_DefendTheBrahmin)
EndFunction

Function Fragment_Stage_0205_Item_00()
	StartCaravanWave("Checkpoint01_Flank")
EndFunction

Function Fragment_Stage_0250_Item_00()
	StopCheckpoint01Waves()
	CompleteOpenObjective(200)
	SetCaravanRefEnabled(alias_R_Wall01, False)
	SetCaravanRefEnabled(alias_R_Wall01_02, False)
	AdvanceCaravanTo(alias_B1_CP02, alias_B2_CP02, alias_G_CP02, alias_M_CP02)
	SetCaravanHalted(False)
	StartCaravanScene(scene_TraveltoCP02)
	SetCaravanStage(76)
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetCaravanHalted(True)
EndFunction

Function Fragment_Stage_0301_Item_00()
	StartCaravanWave("Checkpoint02")
	SetObjectiveDisplayed(300, True, True)
	SetCaravanStage(302)
EndFunction

Function Fragment_Stage_0302_Item_00()
	ShowCaravanMessage(E05_Caravan_Message_DefendTheBrahmin)
EndFunction

Function Fragment_Stage_0303_Item_00()
	StopCaravanWave("Checkpoint02")
	If !IsObjectiveDisplayed(300)
		SetObjectiveDisplayed(300, True, True)
	EndIf
EndFunction

Function Fragment_Stage_0305_Item_00()
	StopCaravanWave("Checkpoint02")
	SetCaravanRefEnabled(alias_R_Bomb02, True)
	; The guard walks back to his checkpoint-02 mark; that marker's distance check sets stage 310.
	PointCaravanMarker(alias_G_CP_CURRENT, alias_G_CP02)
EndFunction

Function Fragment_Stage_0310_Item_00()
	Quest owner = Self as Quest
	If (owner as Quests:E05_Caravan:QuestScript) != None
		(owner as Quests:E05_Caravan:QuestScript).DetonateCaravanBarricade()
	EndIf

	If alias_R_Barricade02 != None && alias_R_Barricade02.GetReference() != None && !alias_R_Barricade02.GetReference().IsDestroyed()
		alias_R_Barricade02.GetReference().DamageObject(1000000.0)
	EndIf
	SetCaravanRefEnabled(alias_R_Wall02, False)
	SetCaravanRefEnabled(alias_R_Bomb02, False)
	SetCaravanStage(350)
EndFunction

Function Fragment_Stage_0350_Item_00()
	CompleteOpenObjective(300)
	AdvanceCaravanTo(alias_B1_CP03, alias_B2_CP03, alias_G_CP03, alias_M_CP03)
	SetCaravanHalted(False)
	StartCaravanScene(scene_TraveltoCP03)
EndFunction

Function Fragment_Stage_0400_Item_00()
	SetCaravanHalted(True)
	StartCaravanWave("Checkpoint03_Back")
	StartCaravanWave("Checkpoint03_Mid")
	StartCaravanWave("Checkpoint03_BossMinions")
	StartCaravanWave("Checkpoint 03 - CaptainBoss")
	SetObjectiveDisplayed(400, True, True)
	SetObjectiveDisplayed(450, True)
	ShowCaravanMessage(E05_Caravan_Message_DefeatTheCaptain)
EndFunction

Function Fragment_Stage_0410_Item_00()
	StartCaravanWave("Checkpoint03_Building")
EndFunction

Function Fragment_Stage_0425_Item_00()
	Quest owner = Self as Quest
	If (owner as Quests:E05_Caravan:QuestScript) != None
		(owner as Quests:E05_Caravan:QuestScript).OpenCargoGate()
	EndIf

	CompleteOpenObjective(450)
	CheckCheckpoint03Objectives()
EndFunction

Function Fragment_Stage_0450_Item_00()
	CompleteOpenObjective(400)
	CheckCheckpoint03Objectives()
EndFunction

Function Fragment_Stage_0460_Item_00()
	If CountLivingCollection(alias_Col_RaidersCP03) > 0
		SetObjectiveDisplayed(900, True, True)
	Else
		SetCaravanStage(475)
	EndIf
EndFunction

Function Fragment_Stage_0475_Item_00()
	StopCheckpoint03Waves()
	CompleteOpenObjective(900)
	CompleteOpenObjective(400)
	CompleteOpenObjective(450)
	AdvanceCaravanTo(alias_B1_CP04, alias_B2_CP04, alias_G_CP04, alias_M_CP04)
	SetCaravanHalted(False)
	StartCaravanScene(scene_TraveltoCP04)
EndFunction

Function Fragment_Stage_0480_Item_00()
	StartCaravanWave("Checkpoint03_Dogs")
EndFunction

Function Fragment_Stage_0525_Item_00()
	SetCaravanHalted(True)
	StartCaravanWave("Checkpoint04_FrontSniper")
	StartCaravanWave("Checkpoint04_BackSnipers")
	StartCaravanWave("Checkpoint04_ShortRange")
	StartCaravanWave("Checkpoint04_Explosive")
	SetObjectiveDisplayed(500, True, True)
	SetCaravanStage(526)
EndFunction

Function Fragment_Stage_0526_Item_00()
	ShowCaravanMessage(E05_Caravan_Message_DefendTheBrahmin)
EndFunction

; Two of the checkpoint-04 waves are endless; stopping them here lets the front collection empty
; into stage 550 once the timed waves are done.
Function Fragment_Stage_0530_Item_00()
	StopCaravanWave("Checkpoint04_BackSnipers")
	StopCaravanWave("Checkpoint04_ShortRange")
	If !IsObjectiveDisplayed(500)
		SetObjectiveDisplayed(500, True, True)
	EndIf
EndFunction

; Debug stage: console-only shortcut to checkpoint 04. The checkpoint-03 stages have to be marked
; first, because the checkpoint-04 marker only registers its arrival check once stage 450 is done.
Function Fragment_Stage_0540_Item_00()
	SetCaravanStage(425)
	SetCaravanStage(450)
	SetCaravanStage(475)
EndFunction

Function Fragment_Stage_0550_Item_00()
	StopAllCaravanWaves()
	CompleteOpenObjective(500)
	CompleteOpenObjective(150)
	SetCaravanRefEnabled(alias_R_Wall04, False)
	SetCaravanRefEnabled(alias_R_Wall04_02, False)
	AdvanceCaravanTo(alias_B1_CP05, alias_B2_CP05, alias_G_CP05, alias_M_CP05)
	SetCaravanHalted(False)
	StartCaravanScene(scene_TraveltoCP05)
	SetObjectiveDisplayed(175, True, True)
	SetCaravanStage(551)
EndFunction

Function Fragment_Stage_0551_Item_00()
	ShowCaravanMessage(E05_Caravan_Message_Escape)
	; FO76 made the brahmin invulnerable for the run to the exit so the escape cannot be lost.
	SetCaravanBrahminGhost(True)
	SetCaravanStage(552)
EndFunction

Function Fragment_Stage_0552_Item_00()
	If alias_door_Exit != None && alias_door_Exit.GetReference() != None
		alias_door_Exit.GetReference().BlockActivation(True)
	EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
	CompleteOpenObjective(175)
	CompleteOpenObjective(100)
	SetCaravanStage(8999)
EndFunction

Function Fragment_Stage_1000_Item_00()
	RefreshCollectedSupplyCount()
EndFunction

Function Fragment_Stage_1001_Item_00()
	RefreshCollectedSupplyCount()
EndFunction

Function Fragment_Stage_1002_Item_00()
	RefreshCollectedSupplyCount()
EndFunction

Function Fragment_Stage_1003_Item_00()
	RefreshCollectedSupplyCount()
EndFunction

; FO76 recorded the run's success on a player actor value for its reward tiers; in FO4 the reward
; rows fire from stage 9000, so this stage only carries the run into completion.
Function Fragment_Stage_8999_Item_00()
	SetCaravanStage(9000)
EndFunction

Function Fragment_Stage_9000_Item_00()
	RefreshCollectedSupplyCount()
	CompleteOpenCaravanObjectives()
	CleanUpCaravan()
	Stop()
EndFunction

Function Fragment_Stage_9001_Item_00()
	FailOpenObjective(2001)
	SetCaravanStage(9005)

	Actor backBrahmin = None
	If alias_Brahmin02 != None
		backBrahmin = alias_Brahmin02.GetActorReference()
	EndIf

	If backBrahmin != None && !backBrahmin.IsDead()
		; Promote the surviving brahmin: the checkpoint distance checks re-register on this alias.
		; The follower alias keeps the dead brahmin so the travel scenes still find their actor.
		If alias_BrahminLeader != None
			alias_BrahminLeader.ForceRefTo(backBrahmin)
		EndIf
	Else
		SetCaravanStage(9990)
	EndIf
EndFunction

Function Fragment_Stage_9002_Item_00()
	FailOpenObjective(2002)
	SetCaravanStage(9006)

	Actor frontBrahmin = None
	If alias_Brahmin01 != None
		frontBrahmin = alias_Brahmin01.GetActorReference()
	EndIf

	If frontBrahmin == None || frontBrahmin.IsDead()
		SetCaravanStage(9990)
	EndIf
EndFunction

Function Fragment_Stage_9003_Item_00()
	ShowCaravanMessage(E05_Caravan_Message_BrahminHurt)
EndFunction

Function Fragment_Stage_9004_Item_00()
	ShowCaravanMessage(E05_Caravan_Message_BrahminHurt)
EndFunction

Function Fragment_Stage_9005_Item_00()
	ShowCaravanMessage(E05_Caravan_Message_BrahminDead)
EndFunction

Function Fragment_Stage_9006_Item_00()
	ShowCaravanMessage(E05_Caravan_Message_BrahminDead)
EndFunction

Function Fragment_Stage_9990_Item_00()
	FailOpenCaravanObjectives()
	CleanUpCaravan()
	Stop()
EndFunction

; Set by B21:QuestTimer when QuestTimerLengthMax (E05_Caravan_Timer_Event, 1500 s) runs out.
Function Fragment_Stage_9992_Item_00()
	SetCaravanStage(9990)
EndFunction

; Run on stop, so it runs exactly once per run: repeat the cleanup (the success and failure stages
; already did it before calling Stop) and hand the next run the next merchant/guard pair.
Function Fragment_Stage_10000_Item_00()
	CleanUpCaravan()

	Quest owner = Self as Quest
	If (owner as Quests:E05_Caravan:QuestScript) != None
		(owner as Quests:E05_Caravan:QuestScript).AdvanceCaravanSchedule()
	EndIf
EndFunction
