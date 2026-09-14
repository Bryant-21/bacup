Event OnEntryRun(Int auiEntryID, ObjectReference akTarget, Actor akOwner)
	If akOwner == Game.GetPlayer() && SFL02_Track_VertibotQuest != None
		If SFL02_Track_VertibotQuest.GetCurrentStageID() < 200
			SFL02_Track_VertibotQuest.SetStage(200)
		EndIf
	EndIf
EndEvent
