Bool Function IsEventOver()
	Return IsStageDone(9000) || IsStageDone(9991)
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

Function CompleteOpenObjectives()
	CompleteOpenObjective(10)
	CompleteOpenObjective(20)
	CompleteOpenObjective(25)
	CompleteOpenObjective(30)
	CompleteOpenObjective(40)
	CompleteOpenObjective(45)
	CompleteOpenObjective(50)
	CompleteOpenObjective(60)
	CompleteOpenObjective(65)
	CompleteOpenObjective(70)
	CompleteOpenObjective(130)
	CompleteOpenObjective(140)
EndFunction

Function FailOpenObjectives()
	FailOpenObjective(10)
	FailOpenObjective(20)
	FailOpenObjective(25)
	FailOpenObjective(30)
	FailOpenObjective(40)
	FailOpenObjective(45)
	FailOpenObjective(50)
	FailOpenObjective(60)
	FailOpenObjective(65)
	FailOpenObjective(70)
	FailOpenObjective(130)
	FailOpenObjective(140)
EndFunction

Function SetAliasEnabled(ReferenceAlias akAlias, Bool abEnabled)
	If akAlias == None || akAlias.GetReference() == None
		Return
	EndIf
	If abEnabled
		akAlias.GetReference().Enable(False)
	Else
		akAlias.GetReference().Disable(False)
	EndIf
EndFunction

Function SetSirenEnabled(Bool abEnabled)
	SetAliasEnabled(Sound_siren, abEnabled)
EndFunction

Function SetStomperState(String asState)
	If Static_Stomper == None
		Return
	EndIf
	Burn_E02_StompingAnimationScript stomper = Static_Stomper.GetReference() as Burn_E02_StompingAnimationScript
	If stomper != None
		stomper.GoToState(asState)
	EndIf
EndFunction

Function StartEventWave(String asWaveID)
	Quest owner = Self as Quest
	DefaultQuestEncounterWaveScript waveScript = owner as DefaultQuestEncounterWaveScript
	If waveScript != None
		waveScript.StartEncounterWaveByID(asWaveID)
	EndIf
EndFunction

Function StopEventWave(String asWaveID)
	Quest owner = Self as Quest
	DefaultQuestEncounterWaveScript waveScript = owner as DefaultQuestEncounterWaveScript
	If waveScript != None
		waveScript.StopEncounterWaveByID(asWaveID, False)
	EndIf
EndFunction

Function StopAllEventWaves()
	Quest owner = Self as Quest
	DefaultQuestEncounterWaveScript waveScript = owner as DefaultQuestEncounterWaveScript
	If waveScript != None
		waveScript.StopAllEncounterWaves(False)
	EndIf
EndFunction

Function StartSwarmWaves(Bool abIncludeFourthWave, Bool abIncludeSecondStingWings)
	StartEventWave("Burning Radscorpion Wave01")
	StartEventWave("Burning Radscorpion Wave02")
	StartEventWave("Burning Radscorpion Wave03")
	If abIncludeFourthWave
		StartEventWave("Burning Radscorpion Wave04")
	EndIf
	StartEventWave("Sinkhole - Spawns - StingWings01")
	If abIncludeSecondStingWings
		StartEventWave("Sinkhole - Spawns - StingWings02")
	EndIf
EndFunction

Function StopSwarmWaves()
	StopEventWave("Burning Radscorpion Wave01")
	StopEventWave("Burning Radscorpion Wave02")
	StopEventWave("Burning Radscorpion Wave03")
	StopEventWave("Burning Radscorpion Wave04")
	StopEventWave("Sinkhole - Spawns - StingWings01")
EndFunction

Function StopNestWaves()
	StopEventWave("Sinkhole - Spawns - StingWings02")
	StopEventWave("Burning Radscorpion Nests")
	StopEventWave("Burning Radscorpion Nests02")
EndFunction

Function BeginNestPhase(Int aiNestObjective, String asCountName, String asTargetName, DefaultCounterQuest akCounter)
	; Objective text reads <Variable=...>; publish 0/target before the nests can be destroyed.
	Quest owner = Self as Quest
	B21:QuestVariables variables = owner as B21:QuestVariables
	If variables != None
		Int target = 0
		If akCounter != None
			target = akCounter.TargetValue
		EndIf
		variables.SetVariable(asCountName, 0.0)
		variables.SetVariable(asTargetName, target as Float)
	EndIf
	SetObjectiveDisplayed(aiNestObjective, True, True)
EndFunction

Function ShowNestMessage(Message akMessage)
	If akMessage != None && !IsEventOver()
		akMessage.Show()
	EndIf
EndFunction

Function GrantCompletionValue()
	Actor playerRef = Game.GetPlayer()
	If playerRef == None || Sinkhole_CompletionAV == None
		Return
	EndIf
	Quest owner = Self as Quest
	DefaultEventQuest eventQuest = owner as DefaultEventQuest
	If eventQuest != None && !eventQuest.IsPlayerParticipating()
		Return
	EndIf
	playerRef.SetValue(Sinkhole_CompletionAV, 1.0)
EndFunction

Function DisableHazardCollection(RefCollectionAlias akCollection)
	If akCollection != None
		akCollection.DisableAll(False)
	EndIf
EndFunction

Function ShutdownEvent()
	CancelTimer(80960)
	If IsRunning()
		Stop()
	EndIf
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID == 80960
		ShutdownEvent()
	EndIf
EndEvent

Function Fragment_Stage_0000_Item_00()
	If !IsStageDone(100)
		SetStage(100)
	EndIf
EndFunction

Function Fragment_Stage_0001_Item_00()
	If !IsEventOver() && !IsStageDone(700)
		SetStage(700)
	EndIf
EndFunction

Function Fragment_Stage_0005_Item_00()
	SetObjectiveDisplayed(1, True, True)
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetSirenEnabled(False)
	SetStomperState("Waiting")
	SetAliasEnabled(Activator_Stomper, True)
	SetObjectiveDisplayed(10, True, True)
EndFunction

Function Fragment_Stage_0125_Item_00()
	CompleteOpenObjective(10)
	SetAliasEnabled(Activator_Stomper, False)
	SetStomperState("active")
	If !IsEventOver()
		SetObjectiveDisplayed(20, True, True)
	EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
	CompleteOpenObjective(20)
	If IsEventOver()
		Return
	EndIf
	SetObjectiveDisplayed(25, True, True)
	StartSwarmWaves(False, False)
EndFunction

Function Fragment_Stage_0175_Item_00()
	If IsEventOver()
		Return
	EndIf
	SetSirenEnabled(True)
	ShowNestMessage(Nest1)
	If !IsStageDone(200)
		SetStage(200)
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	CompleteOpenObjective(25)
	If IsEventOver()
		Return
	EndIf
	StopSwarmWaves()
	Quest owner = Self as Quest
	BeginNestPhase(30, "nestsDestroyed", "nestsDestroyedTarget", owner as DefaultCounterQuestA)
	StartEventWave("Sinkhole - Spawns - StingWings02")
	StartEventWave("Burning Radscorpion Nests")
	If !IsStageDone(225)
		SetStage(225)
	EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
	CompleteOpenObjective(30)
	StopNestWaves()
	SetSirenEnabled(False)
	If !IsEventOver() && !IsStageDone(300)
		SetStage(300)
	EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
	If !IsEventOver()
		SetObjectiveDisplayed(40, True, True)
	EndIf
EndFunction

Function Fragment_Stage_0350_Item_00()
	CompleteOpenObjective(40)
	If IsEventOver()
		Return
	EndIf
	SetObjectiveDisplayed(45, True, True)
	StartSwarmWaves(True, False)
EndFunction

Function Fragment_Stage_0400_Item_00()
	CompleteOpenObjective(45)
	If IsEventOver()
		Return
	EndIf
	StopSwarmWaves()
	SetSirenEnabled(True)
	Quest owner = Self as Quest
	BeginNestPhase(50, "nestsDestroyed2", "nestsDestroyedTarget2", owner as DefaultCounterQuestB)
	StartEventWave("Sinkhole - Spawns - StingWings02")
	StartEventWave("Burning Radscorpion Nests")
	StartEventWave("Burning Radscorpion Nests02")
	If !IsStageDone(425)
		SetStage(425)
	EndIf
EndFunction

Function Fragment_Stage_0425_Item_00()
	ShowNestMessage(Nest2)
EndFunction

Function Fragment_Stage_0450_Item_00()
	CompleteOpenObjective(50)
	StopNestWaves()
	SetSirenEnabled(False)
	If !IsEventOver() && !IsStageDone(500)
		SetStage(500)
	EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
	If !IsEventOver()
		SetObjectiveDisplayed(60, True, True)
	EndIf
EndFunction

Function Fragment_Stage_0550_Item_00()
	CompleteOpenObjective(60)
	If IsEventOver()
		Return
	EndIf
	SetObjectiveDisplayed(65, True, True)
	StartSwarmWaves(True, True)
EndFunction

Function Fragment_Stage_0600_Item_00()
	CompleteOpenObjective(65)
	If IsEventOver()
		Return
	EndIf
	; StingWings02 keeps running from stage 550 until the nests are cleared at 650.
	StopSwarmWaves()
	SetSirenEnabled(True)
	Quest owner = Self as Quest
	BeginNestPhase(70, "nestsDestroyed3", "nestsDestroyedTarget3", owner as DefaultCounterQuestC)
	StartEventWave("Burning Radscorpion Nests")
	StartEventWave("Burning Radscorpion Nests02")
	If !IsStageDone(625)
		SetStage(625)
	EndIf
EndFunction

Function Fragment_Stage_0625_Item_00()
	ShowNestMessage(Nest3)
EndFunction

Function Fragment_Stage_0650_Item_00()
	CompleteOpenObjective(70)
	StopNestWaves()
	SetSirenEnabled(False)
	If !IsEventOver() && !IsStageDone(700)
		SetStage(700)
	EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
	If !IsEventOver()
		SetObjectiveDisplayed(130, True, True)
	EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
	CompleteOpenObjective(130)
	If IsEventOver()
		Return
	EndIf
	; Objectives go up before spawning: the bosses fill alias 8, which objective 140 targets.
	SetObjectiveDisplayed(140, True, True)
	SetSirenEnabled(True)
	StartEventWave("Sinkhole03 - Boss")
	StartEventWave("Sinkhole03 - Boss02")
	StartEventWave("Sinkhole03 - Boss03")
	StartEventWave("Ogua - 1")
	StartEventWave("Ogua - 2")
EndFunction

Function Fragment_Stage_9000_Item_00()
	If IsStageDone(9991)
		Return
	EndIf
	CompleteOpenObjectives()
	StopAllEventWaves()
	SetSirenEnabled(False)
	GrantCompletionValue()
	If !IsStageDone(9600)
		SetStage(9600)
	EndIf
EndFunction

Function Fragment_Stage_9600_Item_00()
	DisableHazardCollection(RefCol_FireHazard01)
	DisableHazardCollection(RefCol_FireHazard02)
	DisableHazardCollection(RefCol_FireHazard03)
	StartTimer(5.0, 80960)
EndFunction

Function Fragment_Stage_9991_Item_00()
	If IsStageDone(9000)
		Return
	EndIf
	FailOpenObjectives()
	SetSirenEnabled(False)
	If !IsStageDone(9992)
		SetStage(9992)
	EndIf
EndFunction

Function Fragment_Stage_9992_Item_00()
	StopAllEventWaves()
	SetAliasEnabled(Activator_Stomper, False)
	StartTimer(5.0, 80960)
EndFunction
