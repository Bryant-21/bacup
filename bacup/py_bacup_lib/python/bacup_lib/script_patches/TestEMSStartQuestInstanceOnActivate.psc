Event OnActivate(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer() && QuestToStartKeyword != None
		QuestToStartKeyword.SendStoryEventAndWait(akActionRef.GetCurrentLocation(), akActionRef, Self)
	EndIf
EndEvent
