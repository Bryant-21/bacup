Event OnActivate(ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer()
		Return
	EndIf

	TW002_Script owningQuest = GetOwningQuest() as TW002_Script
	If owningQuest == None
		Return
	EndIf

	If owningQuest.IsStageDone(100) && !owningQuest.IsStageDone(owningQuest.QuestStartStage)
		owningQuest.SetStage(owningQuest.QuestStartStage)
	ElseIf owningQuest.IsStageDone(owningQuest.GotTapesStage) && !owningQuest.IsStageDone(1000)
		owningQuest.SetStage(1000)
	EndIf
EndEvent
