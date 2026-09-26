Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
	If OwningQuest == None
		OwningQuest = GetOwningQuest() as E09D_GWWS_QuestScript
	EndIf
	Quest owner = GetOwningQuest()
	If OwningQuest == None || owner == None || (CompleteStage >= 0 && owner.IsStageDone(CompleteStage))
		Return
	EndIf
	OwningQuest.AddScore(Score)
EndEvent
