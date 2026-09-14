Event OnActivate(ObjectReference akActivator)
	Quest owningQuest = GetOwningQuest()
	If akActivator != Alias_Player.GetReference() || !owningQuest
		Return
	EndIf

	If owningQuest.IsStageDone(StageToCheck)
		TWZ03_AlreadyPlaced.Show()
	ElseIf !owningQuest.IsStageDone(HasTargetsStage)
		TWZ03_NoTargets.Show()
	EndIf
EndEvent
