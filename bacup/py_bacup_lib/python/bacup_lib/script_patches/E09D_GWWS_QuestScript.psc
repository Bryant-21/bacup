Event OnQuestInit()
	StopStageTimer()
	ResetScore()
EndEvent

Event OnQuestShutdown()
	StopStageTimer()
	RemoveEventItems()
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID != 65072
		Return
	EndIf
	Int stageToSet = stageOnTimer
	stageOnTimer = -1
	If stageToSet >= 0 && IsRunning() && !IsStageDone(stageToSet)
		SetStage(stageToSet)
	EndIf
EndEvent

Function StartStageTimer(Float afSeconds, Int aiStage)
	CancelTimer(65072)
	stageOnTimer = aiStage
	StartTimer(afSeconds, 65072)
EndFunction

Function StopStageTimer()
	CancelTimer(65072)
	stageOnTimer = -1
EndFunction

Function ResetScore()
	currentScore = 0
	PublishScore()
EndFunction

Int Function GetScore()
	Return currentScore
EndFunction

Bool Function IsScoring()
	; Deposits and bonus kills count from the loot phase (170) until the target is met or the loot clock ends (260).
	Return IsRunning() && IsStageDone(170) && !IsStageDone(ScoreMetStage) && !IsStageDone(260)
EndFunction

Function AddScore(Int aiScore)
	If aiScore <= 0 || !IsScoring()
		Return
	EndIf
	currentScore += aiScore
	PublishScore()
	If currentScore >= ScoreTarget as Int
		SetStage(ScoreMetStage)
	EndIf
EndFunction

Function PublishScore()
	Quest owner = Self as Quest
	B21:QuestVariables questVariables = owner as B21:QuestVariables
	If questVariables == None
		Return
	EndIf
	If CurrentScoreTextVar != ""
		questVariables.SetVariable(CurrentScoreTextVar, currentScore as Float)
	EndIf
	If ScoreTargetTextVar != ""
		questVariables.SetVariable(ScoreTargetTextVar, ScoreTarget)
	EndIf
EndFunction

Function RemoveEventItems()
	; Gunther bucks and both cutouts carry Remove_Item_Keyword; they are worthless outside the round.
	Actor player = Game.GetPlayer()
	If player != None && Remove_Item_Keyword != None
		player.RemoveItem(Remove_Item_Keyword, -1, True)
	EndIf
EndFunction
