Function ChooseAndStartDailyQuest()
	Actor playerRef = Game.GetPlayer()
	If playerRef == None || P01A_Nukashine == None || !P01A_Nukashine.IsCompleted()
		Return
	EndIf

	If D01A_Tipsy == None || D01A_Wasted == None || D01A_Tipsy.IsRunning() || D01A_Wasted.IsRunning()
		Return
	EndIf

	Float lastDailyTime = playerRef.GetValue(D01A_BrewingDailyTimestamp)
	If lastDailyTime > 0.0 && Utility.GetCurrentGameTime() - lastDailyTime < 1.0
		Return
	EndIf

	Quests:U01A_Brewing:DailyScript wastedScript = D01A_Wasted as Quests:U01A_Brewing:DailyScript
	Bool wastedAvailable = wastedScript != None && wastedScript.HasAvailableWastedRecipe(playerRef)
	Int tipsyStreak = playerRef.GetValue(D01A_NumConsecutiveTipsy) as Int
	Bool startWasted = wastedAvailable && (tipsyStreak >= NumConsecutiveTipsyAllowed || Utility.RandomFloat(0.0, 1.0) >= DailyChance)
	Bool startedDaily = False

	If startWasted && D01A_Wasted_StartKeyword != None
		startedDaily = D01A_Wasted_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
		If startedDaily
			playerRef.SetValue(D01A_NumConsecutiveTipsy, 0.0)
		EndIf
	ElseIf D01A_Tipsy_StartKeyword != None
		startedDaily = D01A_Tipsy_StartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
		If startedDaily
			playerRef.SetValue(D01A_NumConsecutiveTipsy, (tipsyStreak + 1) as Float)
		EndIf
	EndIf
EndFunction
