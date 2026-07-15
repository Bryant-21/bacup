Event OnDying(ObjectReference akSenderRef, Actor akKiller)
	If akSenderRef == None
		Return
	EndIf

	If CorpseRefCollection != None
		CorpseRefCollection.AddRef(akSenderRef)
	EndIf

	Actor playerRef = Game.GetPlayer()
	If playerRef != None && BountyLegendaryLL != None
		playerRef.AddItem(BountyLegendaryLL, 1, False)
	EndIf

	Quest bountyQuest = GetOwningQuest()
	If bountyQuest != None
		bountyQuest.CompleteAllObjectives()
		bountyQuest.CompleteQuest()
		bountyQuest.Stop()
	EndIf
EndEvent
