State dormant
	Event OnTriggerEnter(ObjectReference akActionRef)
		If akActionRef != Game.GetPlayer()
			Return
		EndIf
		Int index = 0
		While index < ActivateRef_Keywords.Length
			ObjectReference targetRef = GetLinkedRef(ActivateRef_Keywords[index])
			If targetRef != None
				targetRef.Activate(Self)
			EndIf
			index += 1
		EndWhile
		GoToState("active")
	EndEvent
EndState

State active
	Event OnTriggerLeave(ObjectReference akActionRef)
		If akActionRef == Game.GetPlayer() && isEWSStopper
			Int index = 0
			While index < ActivateRef_Keywords.Length
				ObjectReference targetRef = GetLinkedRef(ActivateRef_Keywords[index])
				If targetRef != None
					targetRef.Activate(Self)
				EndIf
				index += 1
			EndWhile
			GoToState("dormant")
		EndIf
	EndEvent
EndState
