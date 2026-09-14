Function HandleActorValueChange(objectreference akSpeakerRef, objectreference akTargetRef)
	If SetOnSpeaker
		Self.ChangeActorValue(akSpeakerRef)
	EndIf
	If SetOnTarget
		Self.ChangeActorValue(akTargetRef)
	EndIf
	If SetOnNearbyPlayers
		; FO76 GetNearbyPlayers() has no FO4 equivalent. Single-player has exactly
		; one player, so the radius test collapses to a distance check on it.
		Actor thePlayer = Game.GetPlayer()
		If akSpeakerRef != None && thePlayer != None && akSpeakerRef.GetDistance(thePlayer) <= Self.GetNearbyDistance()
			Self.ChangeActorValue(thePlayer as objectreference)
		EndIf
	EndIf
EndFunction
