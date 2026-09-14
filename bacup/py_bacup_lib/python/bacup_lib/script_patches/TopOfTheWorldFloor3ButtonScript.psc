Event OnActivate(ObjectReference akActionRef)
	Actor playerRef = akActionRef as Actor
	If playerRef != Game.GetPlayer()
		Return
	EndIf
	If playerRef.HasKeyword(MTNS01_SignalBoosted_Keyword)
		ObjectReference elevatorRef = GetLinkedRef(LinkCustom01)
		If elevatorRef != None
			elevatorRef.Activate(playerRef)
		EndIf
	ElseIf Dialogue_RDR_Rose.IsRunning() && Loudspeaker != None
		Loudspeaker.Say(Dialogue_RDR_Rose_Button3FailTopic, RDR_Contact_Rose, False, playerRef)
	EndIf
EndEvent
