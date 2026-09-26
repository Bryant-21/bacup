Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
	RemoveRef(akSenderRef)
	MTNM04QuestScript questScript = GetOwningQuest() as MTNM04QuestScript
	If questScript != None
		questScript.RefreshUndesirables()
	EndIf
EndEvent
