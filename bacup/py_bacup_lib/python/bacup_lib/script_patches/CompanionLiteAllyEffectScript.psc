Event OnEffectStart(Actor akTarget, Actor akCaster)
	If akTarget != None && InitialQuest != None && InitialQuestStartKeyword != None
		If !InitialQuest.IsRunning() && !InitialQuest.IsCompleted()
			InitialQuestStartKeyword.SendStoryEventAndWait(akTarget.GetCurrentLocation(), akTarget)
		EndIf
	EndIf
EndEvent
