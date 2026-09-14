Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
	Quest owningQuest = GetOwningQuest()
	If owningQuest == None || akActionRef != Game.GetPlayer()
		Return
	EndIf

	If !owningQuest.IsObjectiveDisplayed(iObjectiveToShowOnFirstRepair)
		owningQuest.SetObjectiveCompleted(iObjectiveToCompleteOnFirstRepair)
		owningQuest.SetObjectiveDisplayed(iObjectiveToShowOnFirstRepair)
	EndIf

	TryToSetStage(akSenderRef, False)
EndEvent
