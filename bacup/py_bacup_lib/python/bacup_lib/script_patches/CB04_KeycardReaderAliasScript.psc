Event OnActivate(ObjectReference akActionRef)
	Actor playerRef = Game.GetPlayer()
	Quest owningQuest = GetOwningQuest()
	If akActionRef == playerRef && owningQuest != None && playerRef.GetItemCount(CB04_Keycard) > 0
		Int currentStage = owningQuest.GetCurrentStageID()
		If currentStage >= 500 && currentStage < StageToSetWhenKeycardUsed
			owningQuest.SetStage(StageToSetWhenKeycardUsed)
		EndIf
	EndIf
EndEvent
