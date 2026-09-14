Event OnAliasInit()
	GoToState("ready")
EndEvent

State ready
	Event OnActivate(ObjectReference akActionRef)
		If akActionRef != Game.GetPlayer()
			Return
		EndIf
		GoToState("busy")
		GetOwningQuest().SetStage(200)
	EndEvent
EndState

State busy
	Event OnActivate(ObjectReference akActionRef)
	EndEvent
EndState
