Event OnActivate(ObjectReference akActionRef)
	If akActionRef != currentPlayer.GetReference() || ChoiceMessage == None
		Return
	EndIf
	Int choice = ChoiceMessage.Show()
	If choice >= 0
		Quest owningQuest = GetOwningQuest()
		If owningQuest != None && iStagetoSet >= 0
			owningQuest.SetStage(iStagetoSet)
		EndIf
	EndIf
EndEvent
