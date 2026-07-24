Event OnDeath(Actor akKiller)
	If myKeyword == None
		Return
	EndIf
	Actor linkedActor = Self.GetLinkedRef(myKeyword) as Actor
	If linkedActor != None && !linkedActor.IsDead()
		linkedActor.Kill()
	EndIf
EndEvent
