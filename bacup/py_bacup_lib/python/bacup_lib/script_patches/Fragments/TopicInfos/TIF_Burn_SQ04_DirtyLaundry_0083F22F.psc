Function Fragment_End(ObjectReference akSpeakerRef)
	Burn_SQ04_Collectables_Script tracker = GetOwningQuest() as Burn_SQ04_Collectables_Script
	If tracker != None
		tracker.HandInReadyBatch()
	EndIf
EndFunction
