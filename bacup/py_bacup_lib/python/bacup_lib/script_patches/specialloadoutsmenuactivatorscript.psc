Event OnActivate(ObjectReference akActionRef)
	Actor playerRef = akActionRef as Actor
	If playerRef != Game.GetPlayer()
		Return
	EndIf
	If MinLevelRequirement == None || playerRef.GetLevel() >= MinLevelRequirement.GetValueInt()
		ClientDisplaySpecialBuildsMenu()
	ElseIf MinLevelNotMetMessage != None
		MinLevelNotMetMessage.Show()
	EndIf
EndEvent

Function ClientDisplaySpecialBuildsMenu()
EndFunction
