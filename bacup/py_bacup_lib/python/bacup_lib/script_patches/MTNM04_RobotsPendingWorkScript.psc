Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer()
		Return
	EndIf
	MTNM04QuestScript questScript = GetOwningQuest() as MTNM04QuestScript
	If questScript != None
		questScript.SendRobotToWork(akSenderRef as Actor)
	EndIf
EndEvent
