Event OnTriggerEnter(ObjectReference akActionRef)
	Actor playerRef = akActionRef as Actor
	If playerRef != Game.GetPlayer() || (TriggerOnce && triggered)
		Return
	EndIf
	COMP_CommentTriggerQuestScript commentQuest = InstancedQuest as COMP_CommentTriggerQuestScript
	If commentQuest == None || commentQuest.Speaker == None || SubtypeToSay == None
		Return
	EndIf
	Actor speakerRef = commentQuest.Speaker.GetActorReference()
	If speakerRef != None
		triggered = True
		speakerRef.SayCustom(SubtypeToSay, None, False, playerRef)
	EndIf
EndEvent
