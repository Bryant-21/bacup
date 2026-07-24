Event OnBegin(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
	If !TriggerOnEnd
		Actor speaker = akSpeakerRef as Actor
		If speaker != None
			If TargetInitiatesCombat
				Game.GetPlayer().StartCombat(speaker)
			Else
				speaker.StartCombat(Game.GetPlayer())
			EndIf
		EndIf
	EndIf
EndEvent

Event OnEnd(ObjectReference akSpeakerRef, Bool abHasBeenSaid)
	If TriggerOnEnd
		Actor speaker = akSpeakerRef as Actor
		If speaker != None
			If TargetInitiatesCombat
				Game.GetPlayer().StartCombat(speaker)
			Else
				speaker.StartCombat(Game.GetPlayer())
			EndIf
		EndIf
	EndIf
EndEvent
