Event OnQuestInit()
	If Trigger == None || PlayerAlias == None
		Return
	EndIf

	ObjectReference triggerRef = Trigger.GetReference()
	Actor playerRef = PlayerAlias.GetActorReference()
	If triggerRef != None && playerRef == Game.GetPlayer()
		triggerRef.Activate(playerRef)
	EndIf
EndEvent
