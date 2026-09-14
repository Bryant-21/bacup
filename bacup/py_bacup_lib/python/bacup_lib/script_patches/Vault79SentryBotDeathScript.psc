Event OnDeath(Actor akKiller)
	If myActorValue == None
		Return
	EndIf

	SetValue(myActorValue, 1.0)
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && playerRef != Self
		playerRef.SetValue(myActorValue, 1.0)
	EndIf
EndEvent
