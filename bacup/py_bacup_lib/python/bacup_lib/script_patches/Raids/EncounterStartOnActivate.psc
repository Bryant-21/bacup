; The Enc06 start trigger is a DefaultActivateSelf ref, so it arrives as its own activator.
Event OnActivate(ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer() && (akActionRef == None || akActionRef != GetReference())
		Return
	EndIf
	Quest owningQuest = GetOwningQuest()
	If owningQuest != None && EncounterStartedStage >= 0 && !owningQuest.IsStageDone(EncounterStartedStage)
		owningQuest.SetStage(EncounterStartedStage)
	EndIf
EndEvent
