Event OnTriggerEnter(ObjectReference akActionRef)
	Quest owningQuest = GetOwningQuest()
	If akActionRef == Game.GetPlayer() && owningQuest != None
		Bool prereqMet = PrereqStage < 0 || owningQuest.IsStageDone(PrereqStage)
		Bool stillActive = TurnOffStage < 0 || !owningQuest.IsStageDone(TurnOffStage)
		If prereqMet && stillActive && !owningQuest.IsStageDone(StageToSet)
			owningQuest.SetStage(StageToSet)
		EndIf
	EndIf
EndEvent
