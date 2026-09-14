Event OnActivate(ObjectReference akActionRef)
	Actor playerRef = Game.GetPlayer()
	Quest owningQuest = GetOwningQuest()
	ObjectReference terminalRef = GetReference()
	If akActionRef != playerRef || owningQuest == None || terminalRef == None
		Return
	EndIf
	If owningQuest.GetCurrentStageID() == 475 && terminalRef.IsLocked() && !owningQuest.GetStageDone(500)
		owningQuest.SetStage(500)
	EndIf
EndEvent
