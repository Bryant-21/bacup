; StageToSet + 1 is the mission's failure stage for the same optional (1805 found / 1806 failed).
Event OnActivate(ObjectReference akActionRef)
	Quest owningQuest = GetOwningQuest()
	If akActionRef != Game.GetPlayer() || owningQuest == None
		Return
	EndIf
	If !owningQuest.IsStageDone(StageToSet) && !owningQuest.IsStageDone(StageToSet + 1)
		owningQuest.SetStage(StageToSet)
	EndIf
EndEvent
