Event OnDeath(Actor akKiller)
	Quest owningQuest = GetOwningQuest()
	If owningQuest != None && owningQuest.IsRunning() && !owningQuest.IsStageDone(100)
		owningQuest.SetStage(100)
	EndIf
EndEvent
