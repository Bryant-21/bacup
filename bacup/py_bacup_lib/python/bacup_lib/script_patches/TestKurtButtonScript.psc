Event OnActivate(ObjectReference akActionRef)
	If !bTriggered && akActionRef == Game.GetPlayer() && QuestToStart != None
		bTriggered = True
		myQuest = QuestToStart
		If !myQuest.IsRunning()
			myQuest.Start()
		EndIf
	EndIf
EndEvent
