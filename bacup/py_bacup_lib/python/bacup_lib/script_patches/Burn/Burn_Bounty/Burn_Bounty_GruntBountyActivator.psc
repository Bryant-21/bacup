Event OnActivate(ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer()
		Return
	EndIf

	If GruntHuntQuest != None && GruntHuntQuest.IsRunning()
		If GruntHuntActiveMessage != None
			GruntHuntActiveMessage.Show()
		EndIf
		Return
	EndIf

	If GruntHuntQuest != None
		GruntHuntQuest.Start()
		If GruntHuntQuest.IsRunning() && GruntHuntStartedMessage != None
			GruntHuntStartedMessage.Show()
		EndIf
	EndIf
EndEvent
