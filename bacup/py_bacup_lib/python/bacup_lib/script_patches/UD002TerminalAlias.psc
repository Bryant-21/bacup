Event OnActivate(ObjectReference akActionRef)
	If akActionRef == Game.GetPlayer()
		GetOwningQuest().SetStage(iStageToSet)
	EndIf
EndEvent
