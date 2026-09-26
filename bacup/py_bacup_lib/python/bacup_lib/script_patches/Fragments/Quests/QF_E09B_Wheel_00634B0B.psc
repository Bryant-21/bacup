E09B_Script Function WheelScript()
	Quest owner = Self as Quest
	Return owner as E09B_Script
EndFunction

E09B_MobWaves Function MobWaves()
	Quest owner = Self as Quest
	Return owner as E09B_MobWaves
EndFunction

Function ResetObjective(Int aiObjective)
	SetObjectiveDisplayed(aiObjective, False)
	SetObjectiveCompleted(aiObjective, False)
	SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetEventObjectives()
	ResetObjective(0)
	ResetObjective(5)
	ResetObjective(10)
	ResetObjective(15)
	ResetObjective(16)
	ResetObjective(17)
	ResetObjective(18)
	ResetObjective(21)
	ResetObjective(22)
	ResetObjective(23)
	ResetObjective(25)
	ResetObjective(50)
	ResetObjective(60)
	ResetObjective(100)
	ResetObjective(101)
	ResetObjective(102)
	ResetObjective(200)
	ResetObjective(300)
	ResetObjective(305)
	ResetObjective(310)
	ResetObjective(320)
	ResetObjective(330)
EndFunction

Function ResetRoundObjectives()
	ResetObjective(16)
	ResetObjective(17)
	ResetObjective(18)
EndFunction

Function CompleteOpenObjective(Int aiObjective)
	If aiObjective >= 0 && IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
		SetObjectiveCompleted(aiObjective, True)
	EndIf
EndFunction

Function FailOpenObjective(Int aiObjective)
	If aiObjective >= 0 && IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
		SetObjectiveFailed(aiObjective, True)
	EndIf
EndFunction

Function ResolveObjective(Int aiObjective, Bool abSucceeded)
	If abSucceeded
		CompleteOpenObjective(aiObjective)
	Else
		FailOpenObjective(aiObjective)
	EndIf
EndFunction

Function FailAllOpenObjectives()
	FailOpenObjective(0)
	FailOpenObjective(5)
	FailOpenObjective(10)
	FailOpenObjective(15)
	FailOpenObjective(16)
	FailOpenObjective(17)
	FailOpenObjective(18)
	FailOpenObjective(21)
	FailOpenObjective(22)
	FailOpenObjective(23)
	FailOpenObjective(25)
	FailOpenObjective(50)
	FailOpenObjective(60)
	FailOpenObjective(100)
	FailOpenObjective(101)
	FailOpenObjective(102)
	FailOpenObjective(200)
	FailOpenObjective(300)
	FailOpenObjective(305)
	FailOpenObjective(310)
	FailOpenObjective(320)
	FailOpenObjective(330)
EndFunction

Int Function TwistObjective(Int aiTwistStage)
	If aiTwistStage == 4030
		Return 330
	ElseIf aiTwistStage == 4050
		Return 300
	ElseIf aiTwistStage == 4090
		Return 320
	ElseIf aiTwistStage == 4100
		Return 305
	ElseIf aiTwistStage == 4120
		Return 310
	ElseIf aiTwistStage == 5100
		Return 50
	ElseIf aiTwistStage == 5120
		Return 60
	EndIf
	Return -1
EndFunction

Function StartRandomScene(Scene[] akScenes)
	If akScenes == None || akScenes.Length == 0
		Return
	EndIf
	Scene chosen = akScenes[Utility.RandomInt(0, akScenes.Length - 1)]
	If chosen != None
		chosen.Start()
	EndIf
EndFunction

Function StartSceneIfPresent(Scene akScene)
	If akScene != None
		akScene.Start()
	EndIf
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

Int Function SetAliasGroupEnabled(ReferenceAlias[] akAliases, Bool abEnabled)
	Int changed = 0
	Int index = 0
	While akAliases != None && index < akAliases.Length
		If akAliases[index] != None && akAliases[index].GetReference() != None
			SetAliasEnabled(akAliases[index], abEnabled)
			changed += 1
		EndIf
		index += 1
	EndWhile
	Return changed
EndFunction

Function RestoreMascot(ReferenceAlias akMascot)
	If akMascot == None || akMascot.GetReference() == None
		Return
	EndIf
	ObjectReference mascotRef = akMascot.GetReference()
	mascotRef.ClearDestruction()
	If NoTargetKeyword != None
		mascotRef.RemoveKeyword(NoTargetKeyword)
	EndIf
EndFunction

Function RemovePinata()
	If Alias_Pinata == None
		Return
	EndIf
	ObjectReference pinataRef = Alias_Pinata.GetReference()
	Alias_Pinata.Clear()
	If pinataRef != None
		pinataRef.DisableNoWait()
		pinataRef.Delete()
	EndIf
EndFunction

Function SpawnPinata()
	RemovePinata()
	If Pinata_Form == None || Alias_Pinata == None
		Return
	EndIf
	ObjectReference spawnPoint = None
	If Alias_VimMachinePoints != None && Alias_VimMachinePoints.GetCount() > 0
		spawnPoint = Alias_VimMachinePoints.GetAt(Utility.RandomInt(0, Alias_VimMachinePoints.GetCount() - 1))
	EndIf
	If spawnPoint == None && Alias_Wheel != None
		spawnPoint = Alias_Wheel.GetReference()
	EndIf
	If spawnPoint == None
		Return
	EndIf
	ObjectReference pinataRef = spawnPoint.PlaceAtMe(Pinata_Form, 1, False, False, False)
	If pinataRef != None
		Alias_Pinata.ForceRefTo(pinataRef)
	EndIf
EndFunction

Function ResetTwistWorld()
	SetAliasGroupEnabled(Group_GoldenCappy, False)
	SetAliasGroupEnabled(Group_ChaseChicken, False)
	SetAliasEnabled(Alias_ImposterHandler, False)
	RemovePinata()
EndFunction

Function StopRoundWave()
	E09B_MobWaves waves = MobWaves()
	If waves != None
		waves.StopWave(True)
	EndIf
EndFunction

Function StartNamedWave(String asWaveID)
	E09B_MobWaves waves = MobWaves()
	If waves != None
		waves.StartWaveByID(asWaveID)
	EndIf
EndFunction

Function FinishTwist()
	If !IsStageDone(9999)
		SetStage(5000)
	EndIf
EndFunction

Function HandleMascotDestroyed(Int aiStage, Int aiObjective, ReferenceAlias akMascot)
	FailOpenObjective(aiObjective)
	If akMascot != None && akMascot.GetReference() != None && NoTargetKeyword != None
		akMascot.GetReference().AddKeyword(NoTargetKeyword)
	EndIf
	; Counted from the three destroyed-mascot stages so a restarted run never inherits DefaultCounterQuest's stale count.
	Int destroyed = 0
	If aiStage == 200 || IsStageDone(200)
		destroyed += 1
	EndIf
	If aiStage == 201 || IsStageDone(201)
		destroyed += 1
	EndIf
	If aiStage == 202 || IsStageDone(202)
		destroyed += 1
	EndIf
	E09B_Script eventScript = WheelScript()
	If eventScript != None
		eventScript.SetQuestVariable("DefencePointsDestroyed", destroyed as Float)
	EndIf
	If destroyed >= 3 && !IsStageDone(9999) && !IsStageDone(10000)
		SetStage(9999)
	EndIf
EndFunction

Function Fragment_Stage_0001_Item_00()
	; QA shortcut: skip the stage trigger and rules and prompt the first spin.
	If !IsStageDone(100)
		SetStage(100)
	EndIf
	If !IsStageDone(160)
		SetStage(160)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	ResetEventObjectives()
	E09B_Script eventScript = WheelScript()
	If eventScript != None
		eventScript.ResetGameState()
	EndIf
	StopRoundWave()
	ResetTwistWorld()
	RestoreMascot(Alias_Mascot_Red)
	RestoreMascot(Alias_Mascot_Yellow)
	RestoreMascot(Alias_Mascot_Blue)
	SetObjectiveDisplayed(0, True, True)
EndFunction

Function Fragment_Stage_0150_Item_00()
	CompleteOpenObjective(0)
	SetObjectiveDisplayed(5, True, True)
	StartSceneIfPresent(PA_STW_Introduction)
	E09B_Script eventScript = WheelScript()
	If eventScript != None
		eventScript.ArmIntroFallback(PA_STW_Introduction)
	EndIf
EndFunction

Function Fragment_Stage_0160_Item_00()
	E09B_Script eventScript = WheelScript()
	If eventScript == None || IsStageDone(9999) || IsStageDone(6000) || eventScript.IsRoundOpen()
		Return
	EndIf
	CompleteOpenObjective(0)
	CompleteOpenObjective(5)
	If !IsObjectiveDisplayed(15)
		SetObjectiveDisplayed(15, True, True)
		SetObjectiveDisplayed(100, True, True)
		SetObjectiveDisplayed(101, True, True)
		SetObjectiveDisplayed(102, True, True)
	EndIf
	Int spinObjective = eventScript.CurrentSpinObjective()
	If spinObjective >= 0
		SetObjectiveDisplayed(spinObjective, True, True)
	EndIf
	eventScript.OpenSpinPrompt()
EndFunction

Function Fragment_Stage_0170_Item_00()
	E09B_Script eventScript = WheelScript()
	If eventScript == None || eventScript.IsRoundOpen() || !eventScript.HasSpinsRemaining()
		Return
	EndIf
	CompleteOpenObjective(eventScript.CurrentSpinObjective())
	If !IsObjectiveDisplayed(200)
		SetObjectiveDisplayed(200, True, True)
	EndIf
	eventScript.SpinWheel()
EndFunction

Function Fragment_Stage_0200_Item_00()
	HandleMascotDestroyed(200, 100, Alias_Mascot_Red)
EndFunction

Function Fragment_Stage_0201_Item_00()
	HandleMascotDestroyed(201, 102, Alias_Mascot_Blue)
EndFunction

Function Fragment_Stage_0202_Item_00()
	HandleMascotDestroyed(202, 101, Alias_Mascot_Yellow)
EndFunction

Function Fragment_Stage_1111_Item_00()
	E09B_Script eventScript = WheelScript()
	If eventScript == None || !eventScript.IsRoundOpen()
		Return
	EndIf
	ResetRoundObjectives()
	eventScript.StartCombatRound(False)
	SetObjectiveDisplayed(16, True, True)
	StartRandomScene(PA_STW_Mob)
EndFunction

Function Fragment_Stage_2222_Item_00()
	E09B_Script eventScript = WheelScript()
	If eventScript == None || !eventScript.IsRoundOpen()
		Return
	EndIf
	ResetRoundObjectives()
	eventScript.StartCombatRound(True)
	SetObjectiveDisplayed(17, True, True)
	StartRandomScene(PA_STW_Boss)
EndFunction

Function Fragment_Stage_3333_Item_00()
	E09B_Script eventScript = WheelScript()
	If eventScript == None || !eventScript.IsRoundOpen()
		Return
	EndIf
	ResetRoundObjectives()
	Int twistStageID = eventScript.StartTwistRound()
	SetObjectiveDisplayed(18, True, True)
	If twistStageID >= 0
		SetStage(twistStageID)
	Else
		FinishTwist()
	EndIf
EndFunction

Function Fragment_Stage_4030_Item_00()
	SetObjectiveDisplayed(330, True, True)
	StartNamedWave("ExplosiveFriends")
	StartSceneIfPresent(PA_Wheel_ExplosiveFreind)
EndFunction

Function Fragment_Stage_4035_Item_00()
	StopRoundWave()
	CompleteOpenObjective(330)
	FinishTwist()
EndFunction

Function Fragment_Stage_4050_Item_00()
	E09B_Script eventScript = WheelScript()
	If eventScript != None
		eventScript.BeginImposter()
	EndIf
	SetAliasEnabled(Alias_ImposterHandler, True)
	SetObjectiveDisplayed(300, True, True)
	StartSceneIfPresent(PA_STW_Imposter)
EndFunction

Function Fragment_Stage_4052_Item_00()
	CompleteOpenObjective(300)
	SetAliasEnabled(Alias_ImposterHandler, False)
	FinishTwist()
EndFunction

Function Fragment_Stage_4055_Item_00()
	FailOpenObjective(300)
	SetAliasEnabled(Alias_ImposterHandler, False)
	FinishTwist()
EndFunction

Function Fragment_Stage_4090_Item_00()
	E09B_Script eventScript = WheelScript()
	If eventScript != None
		eventScript.BeginBrahminTipping()
	EndIf
	SetObjectiveDisplayed(320, True, True)
	StartNamedWave("TippableBrahmin")
	StartSceneIfPresent(PA_STW_Brahmin)
EndFunction

Function Fragment_Stage_4095_Item_00()
	E09B_Script eventScript = WheelScript()
	Bool tippedAll = False
	If eventScript != None
		tippedAll = eventScript.EndBrahminTipping()
	EndIf
	ResolveObjective(320, tippedAll)
	StopRoundWave()
	FinishTwist()
EndFunction

Function Fragment_Stage_4100_Item_00()
	Int chickens = SetAliasGroupEnabled(Group_ChaseChicken, True)
	E09B_Script eventScript = WheelScript()
	If eventScript != None
		eventScript.BeginChickenChase(chickens)
	EndIf
	SetObjectiveDisplayed(305, True, True)
	StartSceneIfPresent(PA_STW_Chicken)
EndFunction

Function Fragment_Stage_4115_Item_00()
	E09B_Script eventScript = WheelScript()
	Bool caughtAll = False
	If eventScript != None
		caughtAll = eventScript.EndChickenChase()
	EndIf
	ResolveObjective(305, caughtAll)
	SetAliasGroupEnabled(Group_ChaseChicken, False)
	FinishTwist()
EndFunction

Function Fragment_Stage_4120_Item_00()
	E09B_Script eventScript = WheelScript()
	If eventScript != None
		eventScript.BeginKeepMoving()
	EndIf
	SetObjectiveDisplayed(310, True, True)
	StartSceneIfPresent(PA_STW_KeepMoving)
EndFunction

Function Fragment_Stage_4130_Item_00()
	E09B_Script eventScript = WheelScript()
	Bool survived = True
	If eventScript != None
		survived = eventScript.EndKeepMoving()
	EndIf
	ResolveObjective(310, survived)
	FinishTwist()
EndFunction

Function Fragment_Stage_5000_Item_00()
	E09B_Script eventScript = WheelScript()
	If eventScript == None || !eventScript.TryClosePostSpin()
		Return
	EndIf

	Bool succeeded = True
	E09B_MobWaves waves = MobWaves()
	If eventScript.CurrentWheelResult() == 0
		succeeded = waves == None || waves.CountLivingWaveActors() <= 0
		ResolveObjective(17, succeeded)
	ElseIf eventScript.CurrentWheelResult() == 1
		ResolveObjective(16, True)
	Else
		Int twistObjective = TwistObjective(eventScript.CurrentTwistStage())
		succeeded = twistObjective < 0 || IsObjectiveCompleted(twistObjective)
		FailOpenObjective(twistObjective)
		ResolveObjective(18, succeeded)
	EndIf
	StopRoundWave()
	eventScript.FinishRound(succeeded)

	If IsStageDone(9999)
		Return
	EndIf
	If eventScript.HasSpinsRemaining()
		StartRandomScene(PA_STW_STW)
		SetStage(160)
	Else
		SetStage(6000)
	EndIf
EndFunction

Function Fragment_Stage_5100_Item_00()
	Int cappyCount = SetAliasGroupEnabled(Group_GoldenCappy, True)
	E09B_Script eventScript = WheelScript()
	If eventScript != None
		eventScript.BeginCappyHunt(cappyCount)
	EndIf
	SetObjectiveDisplayed(50, True, True)
	StartSceneIfPresent(PA_STW_CappyHunt)
EndFunction

Function Fragment_Stage_5105_Item_00()
	CompleteOpenObjective(50)
	E09B_Script eventScript = WheelScript()
	If eventScript != None
		eventScript.EndCappyHunt()
	EndIf
	SetAliasGroupEnabled(Group_GoldenCappy, False)
	FinishTwist()
EndFunction

Function Fragment_Stage_5106_Item_00()
	FailOpenObjective(50)
	E09B_Script eventScript = WheelScript()
	If eventScript != None
		eventScript.EndCappyHunt()
	EndIf
	SetAliasGroupEnabled(Group_GoldenCappy, False)
	FinishTwist()
EndFunction

Function Fragment_Stage_5120_Item_00()
	SpawnPinata()
	SetObjectiveDisplayed(60, True, True)
	StartSceneIfPresent(PA_STW_Pinata)
EndFunction

Function Fragment_Stage_5125_Item_00()
	CompleteOpenObjective(60)
	RemovePinata()
	FinishTwist()
EndFunction

Function Fragment_Stage_5126_Item_00()
	FailOpenObjective(60)
	RemovePinata()
	FinishTwist()
EndFunction

Function Fragment_Stage_6000_Item_00()
	CompleteOpenObjective(15)
	CompleteOpenObjective(100)
	CompleteOpenObjective(101)
	CompleteOpenObjective(102)
	CompleteOpenObjective(200)
	StartRandomScene(PA_STW_Win)
	Int index = 0
	While Alias_FanfareCannon != None && index < Alias_FanfareCannon.GetCount()
		ObjectReference cannon = Alias_FanfareCannon.GetAt(index)
		If cannon != None
			cannon.Activate(cannon)
		EndIf
		index += 1
	EndWhile
	If !IsStageDone(9999) && !IsStageDone(10000)
		SetStage(10000)
	EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
	StopRoundWave()
	FailAllOpenObjectives()
	ResetTwistWorld()
	StartRandomScene(PA_STW_Fail)
	E09B_Script eventScript = WheelScript()
	If eventScript != None
		eventScript.CleanupGame()
		eventScript.ScheduleShutdown(10.0)
	Else
		Stop()
	EndIf
EndFunction

Function Fragment_Stage_10000_Item_00()
	E09B_Script eventScript = WheelScript()
	If eventScript != None
		; The delay lets stage-10000 rewards and the win scene run before shutdown.
		eventScript.ScheduleShutdown(15.0)
	Else
		Stop()
	EndIf
EndFunction

Function Fragment_Stage_15000_Item_00()
	StopRoundWave()
	ResetTwistWorld()
	E09B_Script eventScript = WheelScript()
	If eventScript != None
		eventScript.CleanupGame()
	EndIf
EndFunction
