Event OnAliasInit()
	GoToState("ready")
EndEvent

State ready
	Event OnActivate(ObjectReference akActionRef)
		If akActionRef != Game.GetPlayer()
			Return
		EndIf
		Quest owningQuest = GetOwningQuest()
		; The master shutdown only arms at stage 190, once both access panels are live.
		If owningQuest == None || !owningQuest.GetStageDone(190) || owningQuest.GetStageDone(200)
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
