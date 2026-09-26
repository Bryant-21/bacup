E08B_EvictionNoticeScript Function EventScript()
	Quest owner = Self as Quest
	Return owner as E08B_EvictionNoticeScript
EndFunction

Bool Function IsEventOver()
	Return IsStageDone(9000) || IsStageDone(9998) || IsStageDone(9999)
EndFunction

Function ResetEventObjective(Int aiObjective)
	SetObjectiveDisplayed(aiObjective, False)
	SetObjectiveCompleted(aiObjective, False)
	SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetEventObjectives()
	ResetEventObjective(10)
	ResetEventObjective(20)
	ResetEventObjective(25)
	ResetEventObjective(30)
	ResetEventObjective(40)
	ResetEventObjective(50)
	ResetEventObjective(60)
	ResetEventObjective(70)
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

Function SetScrubberVulnerable(Bool abVulnerable)
	If Alias_RadScrubber == None || CrateIgnoreDamageKeyword == None
		Return
	EndIf
	ObjectReference scrubber = Alias_RadScrubber.GetReference()
	If scrubber == None
		Return
	EndIf
	; The scrubber's destructible data only accepts damage while this keyword is absent.
	If abVulnerable
		scrubber.RemoveKeyword(CrateIgnoreDamageKeyword)
	Else
		scrubber.AddKeyword(CrateIgnoreDamageKeyword)
	EndIf
EndFunction

Function ShutdownEvent(Bool abFailed)
	If abFailed
		FailOpenObjective(10)
		FailOpenObjective(20)
		FailOpenObjective(25)
		FailOpenObjective(30)
		FailOpenObjective(40)
		FailOpenObjective(50)
		FailOpenObjective(60)
	Else
		CompleteOpenObjective(30)
		CompleteOpenObjective(40)
		CompleteOpenObjective(50)
	EndIf
	SetObjectiveDisplayed(70, False)
	SetScrubberVulnerable(False)
	E08B_EvictionNoticeScript eventScript = EventScript()
	If eventScript != None
		eventScript.StopEventWaves()
	EndIf
	; Stage rewards are granted from the same stage's OnStageSet, so the stop waits a moment behind them.
	StartTimer(5.0, 64319)
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID == 64319 && IsRunning()
		Stop()
	EndIf
EndEvent

Function Fragment_Stage_0000_Item_00()
	ResetEventObjectives()
	SetScrubberVulnerable(False)
	E08B_EvictionNoticeScript eventScript = EventScript()
	If eventScript != None
		eventScript.SetRadiationLevel(0)
	EndIf
	SetObjectiveDisplayed(10, True, True)
	If !IsStageDone(100)
		SetStage(100)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	If !IsObjectiveCompleted(10)
		SetObjectiveDisplayed(10, True)
	EndIf
	If PA_EventStartRecording && !PA_EventStartRecording.IsPlaying()
		PA_EventStartRecording.Start()
	EndIf
EndFunction

Function Fragment_Stage_0150_Item_00()
	If IsEventOver()
		Return
	EndIf
	CompleteOpenObjective(10)
	SetObjectiveDisplayed(20, True, True)
EndFunction

Function Fragment_Stage_0170_Item_00()
	If IsEventOver()
		Return
	EndIf
	CompleteOpenObjective(10)
	CompleteOpenObjective(20)
	SetObjectiveDisplayed(25, True, True)
EndFunction

Function Fragment_Stage_0200_Item_00()
	If IsEventOver()
		Return
	EndIf
	CompleteOpenObjective(25)
	; Objective 70 has no text; displaying it arms the 540 s spawn-stop timer that sets stage 650.
	SetObjectiveDisplayed(70, True)
	E08B_EvictionNoticeScript eventScript = EventScript()
	If eventScript != None
		eventScript.SetRadiationLevel(1)
		eventScript.SpawnMeatbags()
	EndIf
	If !IsStageDone(225)
		SetStage(225)
	EndIf
EndFunction

Function Fragment_Stage_0225_Item_00()
	If IsEventOver()
		Return
	EndIf
	SetObjectiveDisplayed(30, True, True)
	E08B_EvictionNoticeScript eventScript = EventScript()
	If eventScript != None
		eventScript.PublishMeatbagCount()
		eventScript.StartMeatbagCombat()
	EndIf
	If !IsStageDone(250)
		SetStage(250)
	EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
	If IsEventOver()
		Return
	EndIf
	SetScrubberVulnerable(True)
	SetObjectiveDisplayed(50, True, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
	If IsEventOver()
		Return
	EndIf
	CompleteOpenObjective(30)
	SetObjectiveDisplayed(40, True, True)
	If !IsStageDone(600)
		SetStage(600)
	EndIf
EndFunction

Function Fragment_Stage_0550_Item_00()
	If IsEventOver() || Alias_RadScrubber == None
		Return
	EndIf
	ObjectReference scrubber = Alias_RadScrubber.GetReference()
	If scrubber != None && scrubber.GetCurrentDestructionStage() > 0
		FailOpenObjective(60)
		SetStage(9999)
	EndIf
EndFunction

Function Fragment_Stage_0555_Item_00()
	CompleteOpenObjective(60)
	If !IsEventOver()
		SetObjectiveDisplayed(50, True)
	EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
	If IsEventOver()
		Return
	EndIf
	If Music_Counterattack != None
		Music_Counterattack.Add()
	EndIf
	E08B_EvictionNoticeScript eventScript = EventScript()
	If eventScript != None
		eventScript.StartCounterattack()
	EndIf
EndFunction

Function Fragment_Stage_0650_Item_00()
	SetObjectiveDisplayed(70, False)
	E08B_EvictionNoticeScript eventScript = EventScript()
	If eventScript != None
		eventScript.StopEventWaves()
	EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
	If IsEventOver()
		Return
	EndIf
	If IsStageDone(500)
		SetStage(9000)
	Else
		SetStage(9998)
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	ShutdownEvent(False)
EndFunction

Function Fragment_Stage_9998_Item_00()
	ShutdownEvent(True)
EndFunction

Function Fragment_Stage_9999_Item_00()
	ShutdownEvent(True)
EndFunction

Function Fragment_Stage_10000_Item_00()
	CancelTimer(64319)
	If Music_Counterattack != None
		Music_Counterattack.Remove()
	EndIf
	SetObjectiveDisplayed(70, False)
	SetScrubberVulnerable(False)
	E08B_EvictionNoticeScript eventScript = EventScript()
	If eventScript != None
		eventScript.CleanupEventWorld()
	EndIf
EndFunction
