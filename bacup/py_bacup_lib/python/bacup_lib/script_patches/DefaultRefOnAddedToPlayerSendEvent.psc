Event OnContainerChanged(ObjectReference akNewContainer, ObjectReference akOldContainer)
	Actor playerRef = Game.GetPlayer()
	If akNewContainer != playerRef as ObjectReference
		Return
	EndIf
	If BlockWhilePlayerIsInCombat && playerRef.IsInCombat()
		Return
	EndIf
	If BlockWhilePlayerIsSitting && playerRef.GetSitState() != 0
		Return
	EndIf
	If BlockWhilePlayerIsInPowerArmor && playerRef.IsInPowerArmor()
		Return
	EndIf
	If QuestCompletionToCheck && QuestCompletionToCheck.IsCompleted()
		Return
	EndIf
	If QuestCompletedActorValue && playerRef.GetValue(QuestCompletedActorValue) >= QuestCompletedMaxCount as Float
		Return
	EndIf
	SendConfiguredStoryEvent(None)
EndEvent
