Event OnTriggerEnter(ObjectReference akActionRef)
	Actor enteringActor = akActionRef as Actor
	If enteringActor != None && enteringActor != Game.GetPlayer()
		enteringActor.Kill()
	EndIf
EndEvent
