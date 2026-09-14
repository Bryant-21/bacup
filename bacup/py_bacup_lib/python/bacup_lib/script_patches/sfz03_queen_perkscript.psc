Event OnEntryRun(Int auiEntryID, ObjectReference akTarget, Actor akOwner)
	SFZ03_Queen_QuestScript hunt = SFZ03_Queen as SFZ03_Queen_QuestScript
	If hunt != None
		hunt.HarvestCryptidSample(akTarget, akOwner)
	EndIf
EndEvent
