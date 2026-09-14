Event OnStoryScript(Keyword akKeyword, Location akLocation, ObjectReference akRef1, ObjectReference akRef2, Int aiValue1, Int aiValue2)
	Debug.Trace("[B21 Daily] DefaultDailyQuest OnStoryScript quest=" + Self as String + " keyword=" + akKeyword as String + " location=" + akLocation as String, 0)
	If akKeyword == SQ_RegionDailyQuestKeyword
		If StartingStage_DailyQuestKeyword >= 0
			Debug.Trace("[B21 Daily] DefaultDailyQuest setting daily stage=" + StartingStage_DailyQuestKeyword as String + " quest=" + Self as String, 0)
			SetStage(StartingStage_DailyQuestKeyword)
			Debug.Trace("[B21 Daily] DefaultDailyQuest daily stage result currentStage=" + GetStage() as String + " running=" + IsRunning() as String, 0)
		Else
			Debug.Trace("[B21 Daily] DefaultDailyQuest ignored daily event: starting stage is negative", 0)
		EndIf
	ElseIf StartingStage_OtherKeyword >= 0
		Debug.Trace("[B21 Daily] DefaultDailyQuest setting other stage=" + StartingStage_OtherKeyword as String + " quest=" + Self as String, 0)
		SetStage(StartingStage_OtherKeyword)
	Else
		Debug.Trace("[B21 Daily] DefaultDailyQuest ignored unmatched story event keyword=" + akKeyword as String, 0)
	EndIf
EndEvent
