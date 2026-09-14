Event OnActivate(ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer()
		Return
	EndIf
	If QuestReq != None && QuestReq.IsRunning()
		Int currentStage = QuestReq.GetStage()
		If currentStage >= iStageMin && currentStage <= iStageMax && ObjRefToTPTo != None
			akActionRef.MoveTo(ObjRefToTPTo)
			Return
		EndIf
	EndIf
	If InaccessibleMessage != None
		InaccessibleMessage.Show()
	EndIf
EndEvent
