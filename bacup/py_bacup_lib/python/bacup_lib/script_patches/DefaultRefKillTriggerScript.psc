Event OnTriggerEnter(ObjectReference akActionRef)
	Actor targetActor = akActionRef as Actor
	If targetActor == None
		Return
	EndIf
	If bKillPlayersOnly && targetActor != Game.GetPlayer()
		Return
	EndIf
	If LinkRefKeyword != None && Self.GetLinkedRef(LinkRefKeyword) == targetActor
		Return
	EndIf
	targetActor.Kill()
	If targetActor.IsDead()
		targetActor.SetValue(DeathQuest_CorpseInaccessible, 1.0)
	EndIf
EndEvent
