Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
	Actor companionActor
	If akSpeakerRef == Game.GetPlayer()
		companionActor = (akSpeakerRef as Actor).GetDialogueTarget()
	Else
		companionActor = akSpeakerRef as Actor
	EndIf
	CompanionScript companion = companionActor as CompanionScript
	If !companion
		Return
	EndIf

	COMP_RQ_Script radiantQuest = companion.GetCurrentRadiantQuest()
	If !radiantQuest || !radiantQuest.IsRunning()
		Return
	EndIf
	If !radiantQuest.IsStageDone(radiantQuest.SecondObjective)
		Return
	EndIf
	If !radiantQuest.IsStageDone(radiantQuest.QuestStageReturnToQuestGiver)
		radiantQuest.SetStage(radiantQuest.QuestStageReturnToQuestGiver)
	EndIf
EndEvent
