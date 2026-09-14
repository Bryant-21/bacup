Event OnActivate(ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer()
		Return
	EndIf
	Quest owningQuest = GetOwningQuest()
	If owningQuest != None && EncounterStartedStage >= 0 && !owningQuest.IsStageDone(EncounterStartedStage)
		owningQuest.SetStage(EncounterStartedStage)
	EndIf
EndEvent
