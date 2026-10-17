Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
	; The target quest is event scoped, so it can only be started through its Story Manager keyword.
	If !QuestToStart
		Return
	EndIf
	If !QuestToStart.IsRunning() && QuestStartKeyword
		QuestStartKeyword.SendStoryEventAndWait(None, Game.GetPlayer() as ObjectReference, akSpeakerRef, 0, 0)
	EndIf
	If QuestToStart.IsRunning() && StageToSet >= 0
		QuestToStart.SetStage(StageToSet)
	EndIf
EndEvent
