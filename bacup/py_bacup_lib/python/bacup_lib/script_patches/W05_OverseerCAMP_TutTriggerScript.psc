Event OnTriggerEnter(ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer() || !QuestData
		Return
	EndIf

	int index = 0
	While index < QuestData.Length
		QuestDatum currentQuest = QuestData[index]
		If currentQuest.BlockingValue && currentQuest.StartingKeyword && currentQuest.TargetQuest
			If akActionRef.GetValue(currentQuest.BlockingValue) <= 0.0 && !currentQuest.TargetQuest.IsRunning() && !currentQuest.TargetQuest.IsCompleted()
				currentQuest.StartingKeyword.SendStoryEvent(akRef1 = akActionRef)
			EndIf
		EndIf
		index += 1
	EndWhile
EndEvent
