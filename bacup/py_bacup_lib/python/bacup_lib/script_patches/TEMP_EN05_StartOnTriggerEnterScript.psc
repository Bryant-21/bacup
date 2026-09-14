Event OnTriggerEnter(ObjectReference akActionRef)
	Actor playerRef = akActionRef as Actor
	If playerRef != Game.GetPlayer()
		Return
	EndIf
	If ActiveQuestKeyword != None && playerRef.HasKeyword(ActiveQuestKeyword)
		Return
	EndIf
	If StoryEventToSend != None
		StoryEventToSend.SendStoryEventAndWait(playerRef.GetCurrentLocation(), playerRef, Self, Value1)
	ElseIf QuestToStart != None && !QuestToStart.IsRunning()
		QuestToStart.Start()
	EndIf
EndEvent
