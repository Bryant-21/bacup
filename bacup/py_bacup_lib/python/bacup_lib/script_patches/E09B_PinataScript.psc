Function GrantPrize()
	If CapsForm == None || maxPrizePool <= 0
		Return
	EndIf
	Quest owningQuest = GetOwningQuest()
	DefaultEventQuest eventQuest = owningQuest as DefaultEventQuest
	If eventQuest != None && !eventQuest.IsPlayerParticipating()
		Return
	EndIf
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		; FO76 scattered the prize pool for every nearby player; the lone participant receives one draw from it.
		playerRef.AddItem(CapsForm, Utility.RandomInt(1, maxPrizePool))
	EndIf
EndFunction

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
	ObjectReference pinataRef = GetReference()
	Quest owningQuest = GetOwningQuest()
	If pinataRef == None || owningQuest == None || !owningQuest.IsRunning() || !pinataRef.IsDestroyed()
		Return
	EndIf
	If postPinataStage < 0 || owningQuest.IsStageDone(postPinataStage)
		Return
	EndIf
	GrantPrize()
	owningQuest.SetStage(postPinataStage)
EndEvent
