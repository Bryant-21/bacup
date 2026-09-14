Event OnQuestInit()
	Actor playerRef = Alias_Player.GetReference() as Actor
	If playerRef == None || MTRZ01QuestKeyword == None
		Return
	EndIf
	If MTRZ01_QuestStarted == None || playerRef.GetValue(MTRZ01_QuestStarted) <= 0.0
		MTRZ01QuestKeyword.SendStoryEventAndWait(Alias_Location.GetLocation(), playerRef, Alias_Key.GetReference())
		If MTRZ01_QuestStarted != None
			playerRef.SetValue(MTRZ01_QuestStarted, 1.0)
		EndIf
	EndIf
EndEvent
