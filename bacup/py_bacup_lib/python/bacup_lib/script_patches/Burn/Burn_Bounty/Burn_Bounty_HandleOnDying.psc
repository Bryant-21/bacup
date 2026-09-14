Event OnDying(ObjectReference akSenderRef, Actor akKiller)
	If akSenderRef == None
		Return
	EndIf

	If CorpseRefCollection != None
		CorpseRefCollection.AddRef(akSenderRef)
	EndIf

	Quest bountyQuest = GetOwningQuest()
	If IsGruntHunt
		If bountyQuest != None && !bountyQuest.IsStageDone(300)
			bountyQuest.SetStage(300)
		EndIf
		Return
	EndIf

	Actor playerRef = Game.GetPlayer()
	If playerRef != None && BountyLegendaryLL != None
		playerRef.AddItem(BountyLegendaryLL, 1, False)
	EndIf

	If bountyQuest != None
		bountyQuest.CompleteAllObjectives()
		bountyQuest.CompleteQuest()
		bountyQuest.Stop()
	EndIf
EndEvent
