Event OnUnload()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && playerRef.GetValue(CampObjectAvailable) > 0.0
		Disable()
	EndIf
EndEvent
