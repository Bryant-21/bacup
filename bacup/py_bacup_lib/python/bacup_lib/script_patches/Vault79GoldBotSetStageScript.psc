Event OnTriggerEnter(ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer()
		Return
	EndIf

	Quest owningQuest = GetOwningQuest()
	If owningQuest != None && !owningQuest.IsStageDone(500)
		owningQuest.SetStage(500)
	EndIf
EndEvent
