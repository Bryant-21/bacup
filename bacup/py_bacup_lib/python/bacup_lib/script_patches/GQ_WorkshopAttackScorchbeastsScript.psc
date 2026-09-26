Event OnQuestInit()
	ScorchbeastCount = RequiredScorchbeastCount()
	SetObjectiveDisplayed(10, False)
	SetObjectiveCompleted(10, False)
	SetObjectiveFailed(10, False)
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
	; SQ_ScorchbeastMasterScript owns the flight in (NoScorchbeastStage 20 /
	; ScorchbeastNearTargetStage 30). This only decides if it never does.
	StartTimer(30.0, 201)
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender != Game.GetPlayer() || !IsRunning()
		Return
	EndIf
	If IsStageDone(ScorchbeastsDeadStage) || IsStageDone(20)
		Return
	EndIf
	If IsStageDone(30)
		StartTimer(3.0, 202)
		StartTimer(90.0, 203)
	Else
		StartTimer(30.0, 201)
	EndIf
EndEvent

Event OnQuestShutdown()
	CancelTimer(201)
	CancelTimer(202)
	CancelTimer(203)
	CancelTimer(204)
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 201
		If !IsRunning() || IsStageDone(30) || IsStageDone(20)
			Return
		EndIf
		If LivingScorchbeastCount() > 0
			SetStage(30)
		Else
			SetStage(20)
		EndIf
	ElseIf aiTimerID == 202
		If !IsRunning() || IsStageDone(ScorchbeastsDeadStage) || IsStageDone(20)
			Return
		EndIf
		If HasAnyScorchbeast() && LivingScorchbeastCount() <= 0
			SetStage(ScorchbeastsDeadStage)
		Else
			StartTimer(3.0, 202)
		EndIf
	ElseIf aiTimerID == 203
		; The attack stage ran but no beast was ever collected: end on the
		; authored "no beasts" stage instead of hanging on a dead objective.
		If IsRunning() && !HasAnyScorchbeast() && !IsStageDone(ScorchbeastsDeadStage) && !IsStageDone(20)
			SetStage(20)
		EndIf
	ElseIf aiTimerID == 204
		If IsRunning()
			Stop()
		EndIf
	EndIf
EndEvent

Int Function RequiredScorchbeastCount()
	Int requiredCount = 1
	If ScorchbeastCountData == None
		Return requiredCount
	EndIf
	Actor playerRef = Game.GetPlayer()
	Int playerLevel = 1
	If playerRef != None
		playerLevel = playerRef.GetLevel()
	EndIf
	Int row = 0
	While row < ScorchbeastCountData.Length
		If ScorchbeastCountData[row].minLevel <= playerLevel && ScorchbeastCountData[row].count > requiredCount
			requiredCount = ScorchbeastCountData[row].count
		EndIf
		row += 1
	EndWhile
	Return requiredCount
EndFunction

Bool Function HasAnyScorchbeast()
	Return Alias_Scorchbeasts != None && Alias_Scorchbeasts.GetCount() > 0
EndFunction

Int Function LivingScorchbeastCount()
	If Alias_Scorchbeasts == None
		Return 0
	EndIf
	Int living = 0
	Int index = 0
	While index < Alias_Scorchbeasts.GetCount()
		Actor beast = Alias_Scorchbeasts.GetAt(index) as Actor
		If beast != None && !beast.IsDead()
			living += 1
		EndIf
		index += 1
	EndWhile
	Return living
EndFunction

Function BeginAttack()
	CancelTimer(201)
	SetObjectiveDisplayed(10, True, True)
	StartTimer(3.0, 202)
	StartTimer(90.0, 203)
EndFunction

Function CompleteAttack()
	CancelTimer(202)
	CancelTimer(203)
	If IsObjectiveDisplayed(10) && !IsObjectiveCompleted(10)
		SetObjectiveCompleted(10, True)
	EndIf
	CancelTimer(204)
	StartTimer(10.0, 204)
EndFunction

Function AbortNoBeasts()
	CancelTimer(201)
	CancelTimer(202)
	CancelTimer(203)
	If IsObjectiveDisplayed(10) && !IsObjectiveCompleted(10)
		SetObjectiveFailed(10, True)
	EndIf
	CancelTimer(204)
	StartTimer(2.0, 204)
EndFunction
