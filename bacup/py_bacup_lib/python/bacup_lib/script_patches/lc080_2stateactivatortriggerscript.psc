State inactive
	Event OnTriggerEnter(ObjectReference akActionRef)
		If akActionRef == Game.GetPlayer() && !lock_TriggerState
			lock_TriggerState = True
			ObjectReference linkedRef = GetLinkedRef(LinkedRefToCall)
			If linkedRef != None
				linkedRef.SetOpen(shouldOpen)
			EndIf
			GoToState("active")
			lock_TriggerState = False
		EndIf
	EndEvent
EndState

State active
	Event OnTriggerLeave(ObjectReference akActionRef)
		If akActionRef == Game.GetPlayer() && !lock_TriggerState
			lock_TriggerState = True
			ObjectReference linkedRef = GetLinkedRef(LinkedRefToCall)
			If linkedRef != None
				linkedRef.SetOpen(!shouldOpen)
			EndIf
			GoToState("inactive")
			lock_TriggerState = False
		EndIf
	EndEvent
EndState
