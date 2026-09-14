Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
	Actor companionActor
	If akSpeakerRef == Game.GetPlayer()
		companionActor = (akSpeakerRef as Actor).GetDialogueTarget()
	Else
		companionActor = akSpeakerRef as Actor
	EndIf
	CompanionScript companion = companionActor as CompanionScript
	If companion
		companion.SetRQAcceptanceStage()
	EndIf
EndEvent
