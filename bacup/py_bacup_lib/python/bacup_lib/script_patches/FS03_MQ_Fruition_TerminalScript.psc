Event OnActivate(ObjectReference akActionRef)
	Actor playerRef = Game.GetPlayer()
	Quest owningQuest = GetOwningQuest()
	ObjectReference terminalRef = GetReference()
	If akActionRef != playerRef || owningQuest == None || terminalRef == None
		Return
	EndIf
	If owningQuest.GetStageDone(475) && !owningQuest.GetStageDone(500) && !owningQuest.GetStageDone(600) && terminalRef.IsLocked()
		owningQuest.SetStage(500)
	EndIf
EndEvent
