Function Fragment_End(ObjectReference akSpeakerRef)
	Actor execActor = Alias_Exec_Killable.GetActorReference()
	If execActor != None
		execActor.SetProtected(False)
	EndIf
EndFunction
