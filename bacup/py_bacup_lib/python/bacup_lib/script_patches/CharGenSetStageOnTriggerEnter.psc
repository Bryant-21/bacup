Event OnTriggerEnter(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer()
		Quest owningQuest = GetOwningQuest()
		If owningQuest != None && StageToSet >= 0
			owningQuest.SetStage(StageToSet)
		EndIf
	EndIf
EndEvent
