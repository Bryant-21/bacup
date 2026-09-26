E09D_GWWS_QuestScript Function EventScript()
	Quest owner = Self as Quest
	Return owner as E09D_GWWS_QuestScript
EndFunction

DefaultQuestEncounterWaveScript Function WaveScript()
	Quest owner = Self as Quest
	Return owner as DefaultQuestEncounterWaveScript
EndFunction

Bool Function IsEventOver()
	Return IsStageDone(1100) || IsStageDone(1150) || IsStageDone(1200) || IsStageDone(2000) || IsStageDone(3000) || IsStageDone(4000)
EndFunction

Bool Function IsLootPhaseOver()
	Return IsStageDone(260) || IsEventOver()
EndFunction

Function ResetEventObjective(Int aiObjective)
	SetObjectiveDisplayed(aiObjective, False)
	SetObjectiveCompleted(aiObjective, False)
	SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetEventObjectives()
	ResetEventObjective(10)
	ResetEventObjective(20)
	ResetEventObjective(30)
	ResetEventObjective(40)
	ResetEventObjective(80)
EndFunction

Function CompleteOpenObjective(Int aiObjective)
	If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
		SetObjectiveCompleted(aiObjective, True)
	EndIf
EndFunction

Function FailOpenObjectives()
	Int[] objectives = New Int[5]
	objectives[0] = 10
	objectives[1] = 20
	objectives[2] = 30
	objectives[3] = 40
	objectives[4] = 80
	Int index = 0
	While index < objectives.Length
		Int objective = objectives[index]
		If IsObjectiveDisplayed(objective) && !IsObjectiveCompleted(objective) && !IsObjectiveFailed(objective)
			SetObjectiveFailed(objective, True)
		EndIf
		index += 1
	EndWhile
EndFunction

Function PlayScene(Scene akScene)
	; The PA announcer scenes are Gunther's voice-over; a scene that did not convert is skipped.
	If akScene != None && !akScene.IsPlaying()
		akScene.Start()
	EndIf
EndFunction

Function StartWave(String asWaveID)
	DefaultQuestEncounterWaveScript waves = WaveScript()
	If waves != None
		waves.StartEncounterWaveByID(asWaveID)
	EndIf
EndFunction

Function StopWave(String asWaveID)
	DefaultQuestEncounterWaveScript waves = WaveScript()
	If waves != None
		waves.StopEncounterWaveByID(asWaveID, False)
	EndIf
EndFunction

Function SetLootablesActive(Bool abActive)
	; Alias 72 (Lootables) holds all 56 dispensers and safes, including every value tier's collection.
	If Lootables_Alias == None
		Return
	EndIf
	Int index = 0
	While index < Lootables_Alias.GetCount()
		ObjectReference lootable = Lootables_Alias.GetAt(index)
		If lootable != None
			lootable.BlockActivation(!abActive, !abActive)
		EndIf
		index += 1
	EndWhile
EndFunction

ObjectReference Function PlaceAtRandomMarker(RefCollectionAlias akMarkers, Form akForm)
	If akMarkers == None || akForm == None || akMarkers.GetCount() <= 0
		Return None
	EndIf
	ObjectReference marker = akMarkers.GetAt(Utility.RandomInt(0, akMarkers.GetCount() - 1))
	If marker == None
		Return None
	EndIf
	Return marker.PlaceAtMe(akForm, 1, True, False, False)
EndFunction

Function RemoveCutout(ReferenceAlias akCutoutAlias)
	If akCutoutAlias == None
		Return
	EndIf
	ObjectReference cutout = akCutoutAlias.GetReference()
	akCutoutAlias.Clear()
	; Carried cutouts leave through the Remove_Item_Keyword sweep; only world copies are deleted here.
	If cutout != None && !cutout.IsDeleted() && cutout.GetContainer() == None
		cutout.Disable(False)
		cutout.Delete()
	EndIf
EndFunction

Function ClearRobots(Bool abKill)
	DefaultQuestEncounterWaveScript waves = WaveScript()
	If waves != None
		waves.StopAllEncounterWaves(False)
	EndIf
	; Alias 33 (Wave_GoldenLiberator) pays score and its stage 230 reward on death, so it is only removed.
	RefCollectionAlias goldenVarmints = GetAlias(33) as RefCollectionAlias
	ClearRobotCollection(Robots_Alias, goldenVarmints, abKill)
	ClearRobotCollection(Wave_Sentry, goldenVarmints, abKill)
EndFunction

Function ClearRobotCollection(RefCollectionAlias akRobots, RefCollectionAlias akNeverKill, Bool abKill)
	If akRobots == None
		Return
	EndIf
	Int index = akRobots.GetCount() - 1
	While index >= 0
		Actor robot = akRobots.GetAt(index) as Actor
		If robot != None && !robot.IsDead()
			If abKill && (akNeverKill == None || akNeverKill.Find(robot) < 0)
				robot.Kill()
			Else
				robot.DisableNoWait()
				robot.Delete()
			EndIf
		EndIf
		index -= 1
	EndWhile
EndFunction

Function SetMineOpen(Bool abOpen)
	If Mine_Alias != None && Mine_Alias.GetReference() != None
		Mine_Alias.GetReference().SetOpen(abOpen)
	EndIf
EndFunction

Function ClearHighNoonEffects()
	If Smoke_Alias == None
		Return
	EndIf
	ObjectReference smoke = Smoke_Alias.GetReference()
	Smoke_Alias.Clear()
	If smoke != None
		smoke.Disable(False)
		smoke.Delete()
	EndIf
EndFunction

Function RepairWagon()
	If Wagon != None && Wagon.GetReference() != None
		Wagon.GetReference().ClearDestruction()
	EndIf
	E09D_WagonAliasScript wagonScript = Wagon as E09D_WagonAliasScript
	If wagonScript != None
		wagonScript.ClearSmoke()
	EndIf
EndFunction

Function ScheduleShutdown()
	; Stage rewards are granted from the same stage's OnStageSet, so the stop waits a moment behind them.
	StartTimer(5.0, 65071)
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID == 65071 && IsRunning()
		Stop()
	EndIf
EndEvent

Function Fragment_Stage_0100_Item_00()
	ResetEventObjectives()
	RepairWagon()
	SetLootablesActive(False)
	RemoveCutout(CappyAlias)
	RemoveCutout(BottleAlias)
	SetMineOpen(False)
	ClearHighNoonEffects()
	E09D_GWWS_QuestScript eventScript = EventScript()
	If eventScript != None
		eventScript.ResetScore()
	EndIf
	SetObjectiveDisplayed(10, True, True)
EndFunction

Function Fragment_Stage_0150_Item_00()
	If IsEventOver()
		Return
	EndIf
	CompleteOpenObjective(10)
	SetObjectiveDisplayed(20, True, True)
	PlayScene(PA_WavePrep)
EndFunction

Function Fragment_Stage_0170_Item_00()
	If IsEventOver()
		Return
	EndIf
	CompleteOpenObjective(10)
	CompleteOpenObjective(20)
	SetObjectiveDisplayed(30, True, True)
	SetObjectiveDisplayed(40, True, True)
	SetLootablesActive(True)
	E09D_GWWS_QuestScript eventScript = EventScript()
	If eventScript != None
		eventScript.PublishScore()
	EndIf
	If !IsStageDone(180)
		SetStage(180)
	EndIf
EndFunction

Function Fragment_Stage_0180_Item_00()
	If IsLootPhaseOver()
		Return
	EndIf
	PlayScene(PA_WaveStart)
	StartWave("Wave1_Players_Protectrons")
	StartWave("Wave1_Players_Liberators")
	; The 300 s loot clock (objective 30) spans three waves; each wave hands over after a third of it.
	E09D_GWWS_QuestScript eventScript = EventScript()
	If eventScript != None
		eventScript.StartStageTimer(100.0, 190)
	EndIf
EndFunction

Function Fragment_Stage_0190_Item_00()
	If IsLootPhaseOver()
		Return
	EndIf
	PlayScene(PA_WaveGeneral)
	StopWave("Wave1_Players_Protectrons")
	StartWave("Wave2_Players_Protectrons")
	StartWave("Wave2_Bonus_GoldenLiberator")
	If !IsStageDone(240)
		SetStage(240)
	EndIf
	E09D_GWWS_QuestScript eventScript = EventScript()
	If eventScript != None
		eventScript.StartStageTimer(100.0, 200)
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	If IsLootPhaseOver()
		Return
	EndIf
	PlayScene(PA_WaveCB)
	StopWave("Wave1_Players_Liberators")
	StartWave("Wave3_Players_Liberators")
	StartWave("Wave3_Players_Asaultron")
EndFunction

Function Fragment_Stage_0230_Item_00()
	If !IsEventOver()
		PlayScene(PA_React_GVDeath)
	EndIf
EndFunction

Function Fragment_Stage_0240_Item_00()
	If IsLootPhaseOver()
		Return
	EndIf
	If CappyAlias != None && CappyAlias.GetReference() == None
		ObjectReference cappy = PlaceAtRandomMarker(CappySpawnMarkers, CappyObject)
		If cappy != None
			CappyAlias.ForceRefTo(cappy)
		EndIf
	EndIf
	If BottleAlias != None && BottleAlias.GetReference() == None
		ObjectReference bottle = PlaceAtRandomMarker(BottleSpawnMarkers, BottleObject)
		If bottle != None
			BottleAlias.ForceRefTo(bottle)
		EndIf
	EndIf
EndFunction

Function Fragment_Stage_0241_Item_00()
	If !IsEventOver()
		PlayScene(PA_React_Cappy_Found)
	EndIf
EndFunction

Function Fragment_Stage_0242_Item_00()
	If !IsEventOver()
		PlayScene(PA_React_Bottle_Found)
	EndIf
EndFunction

Function Fragment_Stage_0243_Item_00()
	If !IsEventOver()
		PlayScene(PA_React_Cappy_Deposit)
	EndIf
EndFunction

Function Fragment_Stage_0244_Item_00()
	If !IsEventOver()
		PlayScene(PA_React_Bottle_Deposit)
	EndIf
EndFunction

Function Fragment_Stage_0245_Item_00()
	If !IsEventOver()
		PlayScene(PA_React_BC_Deposit)
	EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
	If IsEventOver()
		Return
	EndIf
	PlayScene(PA_ScoreMet)
	CompleteOpenObjective(30)
	CompleteOpenObjective(40)
	If !IsStageDone(260)
		SetStage(260)
	EndIf
EndFunction

Function Fragment_Stage_0260_Item_00()
	If IsEventOver()
		Return
	EndIf
	E09D_GWWS_QuestScript eventScript = EventScript()
	If eventScript != None
		eventScript.StopStageTimer()
	EndIf
	; Objective 30's loot clock also lands here: without the score the round is lost.
	If !IsStageDone(250)
		SetStage(1200)
		Return
	EndIf
	DefaultQuestEncounterWaveScript waves = WaveScript()
	If waves != None
		waves.StopAllEncounterWaves(False)
	EndIf
	If !IsStageDone(270)
		SetStage(270)
	EndIf
EndFunction

Function Fragment_Stage_0270_Item_00()
	If IsEventOver()
		Return
	EndIf
	SetObjectiveDisplayed(80, True, True)
	If DefenceMessage != None
		DefenceMessage.Show()
	EndIf
	SetLootablesActive(False)
	RemoveCutout(CappyAlias)
	RemoveCutout(BottleAlias)
	E09D_GWWS_QuestScript eventScript = EventScript()
	If eventScript != None
		eventScript.RemoveEventItems()
	EndIf
	If !IsStageDone(280)
		SetStage(280)
	EndIf
EndFunction

Function Fragment_Stage_0280_Item_00()
	If IsEventOver()
		Return
	EndIf
	PlayScene(PA_WaveFinal)
	StartWave("Wave4_Wagon_Protectrons")
	StartWave("Wave4_Wagon_Liberators")
	StartWave("Wave4_Wagon_Asaultron")
	; High noon lands halfway through the 180 s defend clock (objective 80).
	E09D_GWWS_QuestScript eventScript = EventScript()
	If eventScript != None
		eventScript.StartStageTimer(90.0, 290)
	EndIf
EndFunction

Function Fragment_Stage_0290_Item_00()
	If IsEventOver() || IsStageDone(1000)
		Return
	EndIf
	PlayScene(PA_WaveBoss)
	SetMineOpen(True)
	ObjectReference mine = None
	If Mine_Alias != None
		mine = Mine_Alias.GetReference()
	EndIf
	If mine != None && SheriffSmokeFX != None && Smoke_Alias != None && Smoke_Alias.GetReference() == None
		Smoke_Alias.ForceRefTo(mine.PlaceAtMe(SheriffSmokeFX, 1, False, False, False))
	EndIf
	If BellChurchFX != None && Alias_ChurchBell != None && Alias_ChurchBell.GetReference() != None
		BellChurchFX.Play(Alias_ChurchBell.GetReference())
	EndIf

	ObjectReference sheriffMarker = None
	If Alias_SheriffMarker != None
		sheriffMarker = Alias_SheriffMarker.GetReference()
	EndIf
	If sheriffMarker == None || SheriffActor == None || Wave_Sentry == None || Wave_Sentry.GetCount() > 0
		Return
	EndIf
	If SheriffExplosionFX != None
		sheriffMarker.PlaceAtMe(SheriffExplosionFX, 1, False, False, True)
	EndIf
	Actor sheriff = sheriffMarker.PlaceActorAtMe(SheriffActor)
	If sheriff == None
		Return
	EndIf
	; Wave_Sentry carries the sheriff's win-on-death stage (1000), patrol package and wagon targeting.
	Wave_Sentry.AddRef(sheriff)
	Quests:_Default:SetPreferredCombatTargets targeting = Wave_Sentry as Quests:_Default:SetPreferredCombatTargets
	If targeting != None
		targeting.ApplyPreferredCombatTarget(sheriff)
	EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
	If !IsEventOver()
		PlayScene(PA_Wagon75)
	EndIf
EndFunction

Function Fragment_Stage_0510_Item_00()
	If !IsEventOver()
		PlayScene(PA_Wagon50)
	EndIf
EndFunction

Function Fragment_Stage_0520_Item_00()
	If !IsEventOver()
		PlayScene(PA_Wagon25)
	EndIf
EndFunction

Function Fragment_Stage_1000_Item_00()
	If IsEventOver() || !IsStageDone(270)
		Return
	EndIf
	E09D_GWWS_QuestScript eventScript = EventScript()
	If eventScript != None
		eventScript.StopStageTimer()
	EndIf
	PlayScene(PA_End_Timer)
	ClearRobots(True)
	If !IsStageDone(1100)
		SetStage(1100)
	EndIf
EndFunction

Function Fragment_Stage_1100_Item_00()
	If IsStageDone(1150) || IsStageDone(1200) || IsStageDone(2000)
		Return
	EndIf
	PlayScene(PA_End_Victory)
	CompleteOpenObjective(30)
	CompleteOpenObjective(40)
	CompleteOpenObjective(80)
	If !IsStageDone(3000)
		SetStage(3000)
	EndIf
EndFunction

Function Fragment_Stage_1150_Item_00()
	If IsStageDone(1100) || IsStageDone(1200) || IsStageDone(2000)
		Return
	EndIf
	E09D_GWWS_QuestScript eventScript = EventScript()
	If eventScript != None
		eventScript.StopStageTimer()
	EndIf
	PlayScene(PA_End_FailWagon)
	ClearRobots(False)
	FailOpenObjectives()
	If !IsStageDone(4000)
		SetStage(4000)
	EndIf
EndFunction

Function Fragment_Stage_1200_Item_00()
	If IsStageDone(1100) || IsStageDone(1150) || IsStageDone(2000)
		Return
	EndIf
	E09D_GWWS_QuestScript eventScript = EventScript()
	If eventScript != None
		eventScript.StopStageTimer()
	EndIf
	PlayScene(PA_End_FailScore)
	ClearRobots(False)
	FailOpenObjectives()
	If !IsStageDone(4000)
		SetStage(4000)
	EndIf
EndFunction

Function Fragment_Stage_2000_Item_00()
	If IsStageDone(150) || IsStageDone(1100) || IsStageDone(1150) || IsStageDone(1200)
		Return
	EndIf
	FailOpenObjectives()
	If !IsStageDone(4000)
		SetStage(4000)
	EndIf
EndFunction

Function Fragment_Stage_3000_Item_00()
	If !IsStageDone(4000)
		SetStage(4000)
	EndIf
EndFunction

Function Fragment_Stage_4000_Item_00()
	E09D_GWWS_QuestScript eventScript = EventScript()
	If eventScript != None
		eventScript.StopStageTimer()
		eventScript.RemoveEventItems()
	EndIf
	DefaultQuestEncounterWaveScript waves = WaveScript()
	If waves != None
		waves.StopAllEncounterWaves(False)
	EndIf
	SetLootablesActive(False)
	RemoveCutout(CappyAlias)
	RemoveCutout(BottleAlias)
	SetMineOpen(False)
	ClearHighNoonEffects()
	RepairWagon()
	ScheduleShutdown()
EndFunction
