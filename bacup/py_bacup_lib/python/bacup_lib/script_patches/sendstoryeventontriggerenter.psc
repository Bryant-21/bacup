Event OnTriggerEnter(ObjectReference akActionRef)
	Actor playerRef = akActionRef as Actor
	If playerRef != Game.GetPlayer() || QuestStartKeyword == None
		Return
	EndIf
	If QuestCompletedActorValue != None && playerRef.GetValue(QuestCompletedActorValue) > 0.0
		Return
	EndIf
	If QuestActiveKeyword == None || !playerRef.HasKeyword(QuestActiveKeyword)
		QuestStartKeyword.SendStoryEventAndWait(playerRef.GetCurrentLocation(), playerRef, Self)
	EndIf
EndEvent
