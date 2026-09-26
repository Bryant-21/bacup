Quests:E05_Radiation:QuestScript Function GetEventScript()
	Quest owner = Self as Quest
	Return owner as Quests:E05_Radiation:QuestScript
EndFunction

Bool Function IsOperationResolved()
	Return IsStageDone(800) || IsStageDone(900) || IsStageDone(9000) || IsStageDone(9991) || IsStageDone(9992) || IsStageDone(9993)
EndFunction

Bool Function IsOperationFailed()
	Return IsStageDone(800) || IsStageDone(9991) || IsStageDone(9992) || IsStageDone(9993)
EndFunction

Function ResetEventObjective(Int aiObjective)
	SetObjectiveDisplayed(aiObjective, False)
	SetObjectiveCompleted(aiObjective, False)
	SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetEventObjectives()
	ResetEventObjective(10)
	ResetEventObjective(11)
	ResetEventObjective(12)
	ResetEventObjective(13)
	ResetEventObjective(14)
	ResetEventObjective(15)
	ResetEventObjective(16)
	ResetEventObjective(17)
	ResetEventObjective(20)
	ResetEventObjective(40)
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

Function CompleteOperationObjectives()
	CompleteOpenObjective(11)
	CompleteOpenObjective(12)
	CompleteOpenObjective(13)
	CompleteOpenObjective(14)
	CompleteOpenObjective(15)
	CompleteOpenObjective(16)
	CompleteOpenObjective(17)
EndFunction

Function FailOperationObjectives()
	FailOpenObjective(10)
	FailOpenObjective(11)
	FailOpenObjective(12)
	FailOpenObjective(13)
	FailOpenObjective(14)
	FailOpenObjective(15)
	FailOpenObjective(16)
	FailOpenObjective(17)
	FailOpenObjective(20)
EndFunction

Function StopOperationTimer()
	Quest owner = Self as Quest
	B21:QuestTimer questTimer = owner as B21:QuestTimer
	If questTimer != None
		questTimer.StopQuestTimer()
	EndIf
EndFunction

Function StopOperationWaves()
	Quests:E05_Radiation:QuestScript eventScript = GetEventScript()
	If eventScript != None
		eventScript.StopOperationWaves()
	EndIf
EndFunction

Bool Function StartOperationScene(Scene akScene)
	If akScene == None
		Return False
	EndIf
	If !akScene.IsPlaying()
		akScene.Start()
	EndIf
	Return akScene.IsPlaying()
EndFunction

Function ScheduleOperationShutdown(Float afDelay)
	; The closing scene, objective updates and stage rewards land before Stop(); a failure scene may stop the quest sooner.
	StartTimer(afDelay, 5629)
EndFunction

Function FailOperation(Int aiFailureStage, Bool abPlayFailureScene)
	If IsStageDone(900) || IsStageDone(9000)
		Return
	EndIf
	StopOperationTimer()
	StopOperationWaves()
	FailOperationObjectives()
	If abPlayFailureScene
		StartOperationScene(E05_Radiation_FailureScene)
	EndIf
	If !IsStageDone(aiFailureStage)
		SetStage(aiFailureStage)
	EndIf
	ScheduleOperationShutdown(30.0)
EndFunction

Function WatchMarionForBriefing(Bool abWatch)
	If Alias_MarionCopeland == None
		Return
	EndIf
	ObjectReference marionRef = Alias_MarionCopeland.GetReference()
	If marionRef == None
		Return
	EndIf
	If abWatch
		RegisterForRemoteEvent(marionRef, "OnActivate")
	Else
		UnregisterForRemoteEvent(marionRef, "OnActivate")
	EndIf
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer() || Alias_MarionCopeland == None || akSender != Alias_MarionCopeland.GetReference()
		Return
	EndIf
	; No converted topic sets the FO76 briefing stage, so talking to Marion accepts the operation.
	WatchMarionForBriefing(False)
	If IsRunning() && !IsStageDone(200) && !IsOperationResolved()
		SetStage(200)
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 5629
		If IsRunning()
			Stop()
		EndIf
	ElseIf aiTimerID == 5630
		If IsRunning() && IsStageDone(900) && !IsStageDone(9000)
			SetStage(9000)
		EndIf
	EndIf
EndEvent

Function Fragment_Stage_0100_Item_00()
	CancelTimer(5629)
	CancelTimer(5630)
	ResetEventObjectives()
	SetObjectiveDisplayed(10, True, True)
	WatchMarionForBriefing(True)
EndFunction

Function Fragment_Stage_0200_Item_00()
	WatchMarionForBriefing(False)
	If IsOperationResolved()
		Return
	EndIf
	CompleteOpenObjective(10)
	SetObjectiveDisplayed(20, True, True)
	Quests:E05_Radiation:QuestScript eventScript = GetEventScript()
	If eventScript != None
		eventScript.SpawnScavengers()
	EndIf
	StartOperationScene(E05_Radiation_EventInfoScene)
EndFunction

Function Fragment_Stage_0300_Item_00()
	If IsOperationResolved()
		Return
	EndIf
	CompleteOpenObjective(10)
	CompleteOpenObjective(20)
	Quests:E05_Radiation:QuestScript eventScript = GetEventScript()
	If eventScript != None
		eventScript.BeginOperation()
	EndIf
	SetObjectiveDisplayed(11, True, True)
	SetObjectiveDisplayed(12, True)
	SetObjectiveDisplayed(13, True)
	SetObjectiveDisplayed(14, True)
	SetObjectiveDisplayed(15, True)
	SetObjectiveDisplayed(16, True)
	SetObjectiveDisplayed(17, True, True)
	If E05_Radiation_ObjectiveMessage != None
		E05_Radiation_ObjectiveMessage.Show()
	EndIf
EndFunction

Function Fragment_Stage_0400_Item_00()
	Quests:E05_Radiation:QuestScript eventScript = GetEventScript()
	If eventScript != None
		eventScript.HandleOreGoalReached(1)
	EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
	Quests:E05_Radiation:QuestScript eventScript = GetEventScript()
	If eventScript != None
		eventScript.HandleOreGoalReached(2)
	EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
	Quests:E05_Radiation:QuestScript eventScript = GetEventScript()
	If eventScript != None
		eventScript.HandleOreGoalReached(3)
	EndIf
EndFunction

Function Fragment_Stage_0700_Item_00()
	Quests:E05_Radiation:QuestScript eventScript = GetEventScript()
	If eventScript != None
		eventScript.HandleOreGoalReached(4)
	EndIf
	CompleteOpenObjective(17)
EndFunction

Function Fragment_Stage_0750_Item_00()
	If IsOperationResolved()
		Return
	EndIf
	Int survivors = 0
	Quests:E05_Radiation:QuestScript eventScript = GetEventScript()
	If eventScript != None
		survivors = eventScript.CountLivingScavengers()
	EndIf
	If survivors <= 0
		SetStage(800)
	ElseIf IsStageDone(400)
		SetStage(900)
	Else
		SetStage(9993)
	EndIf
EndFunction

Function Fragment_Stage_0800_Item_00()
	FailOperation(9992, True)
EndFunction

Function Fragment_Stage_0900_Item_00()
	If IsOperationFailed()
		Return
	EndIf
	StopOperationTimer()
	StopOperationWaves()
	CompleteOperationObjectives()
	If StartOperationScene(E05_Radiation_SuccessScene)
		; The success scene sets stage 9000 when it completes; this covers a scene that stalls.
		StartTimer(60.0, 5630)
	ElseIf !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	If IsOperationFailed()
		Return
	EndIf
	CancelTimer(5630)
	CompleteOperationObjectives()
	ScheduleOperationShutdown(10.0)
EndFunction

Function Fragment_Stage_9991_Item_00()
	If IsStageDone(9000)
		Return
	EndIf
	StopOperationTimer()
	FailOperationObjectives()
	ScheduleOperationShutdown(5.0)
EndFunction

Function Fragment_Stage_9992_Item_00()
	FailOperation(9992, False)
EndFunction

Function Fragment_Stage_9993_Item_00()
	FailOperation(9993, True)
EndFunction

Function Fragment_Stage_10000_Item_00()
	CancelTimer(5629)
	CancelTimer(5630)
	WatchMarionForBriefing(False)
	Quests:E05_Radiation:QuestScript eventScript = GetEventScript()
	If eventScript != None
		eventScript.CleanupOperation()
	EndIf
EndFunction
