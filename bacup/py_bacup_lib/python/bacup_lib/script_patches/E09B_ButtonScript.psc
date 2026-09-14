Event OnActivate(ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer() || !isFirstPress || GetOwningQuest().GetStage() != 160
		Return
	EndIf
	isFirstPress = False
	ObjectReference buttonRef = GetReference()
	If buttonRef != None
		buttonRef.PlayAnimation("TurnOn01")
	EndIf
	GetOwningQuest().SetStage(170)
EndEvent

Event OnAliasShutdown()
	ObjectReference buttonRef = GetReference()
	If buttonRef != None
		buttonRef.PlayAnimation("TurnOff01")
	EndIf
	isFirstPress = True
EndEvent
