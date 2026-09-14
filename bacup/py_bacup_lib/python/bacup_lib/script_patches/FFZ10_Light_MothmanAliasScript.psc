Function ClearCommunionState()
	Actor playerRef = Game.GetPlayer()
	If playerRef == None
		Return
	EndIf

	If PlayersCanCommune != None && PlayersCanCommune.Find(playerRef) >= 0
		PlayersCanCommune.RemoveRef(playerRef)
	EndIf
	If PlayersAlreadyCommuned != None && PlayersAlreadyCommuned.Find(playerRef) >= 0
		PlayersAlreadyCommuned.RemoveRef(playerRef)
	EndIf
	If FFZ10_Light_CanCommuneKeyword != None
		playerRef.RemoveKeyword(FFZ10_Light_CanCommuneKeyword)
	EndIf
	If FFZ10_Light_AlreadyCommunedKeyword != None
		playerRef.RemoveKeyword(FFZ10_Light_AlreadyCommunedKeyword)
	EndIf
	If FFZ10_Light_WiseMothmanFaction != None
		playerRef.RemoveFromFaction(FFZ10_Light_WiseMothmanFaction)
	EndIf
	If FFZ10_Light_MothmanFriend != None
		playerRef.SetValue(FFZ10_Light_MothmanFriend, 0.0)
	EndIf
EndFunction

Event OnAliasInit()
	ClearCommunionState()
EndEvent

Event OnAliasReset()
	ClearCommunionState()
EndEvent

Event OnAliasShutdown()
	ClearCommunionState()
EndEvent

Event OnActivate(ObjectReference akActionRef)
	Actor playerRef = Game.GetPlayer()
	Quest owningQuest = GetOwningQuest()
	If playerRef == None || akActionRef != playerRef || owningQuest == None
		Return
	EndIf
	If !owningQuest.IsStageDone(200) || owningQuest.IsStageDone(300)
		Return
	EndIf

	Bool alreadyCommuned = PlayersAlreadyCommuned != None && PlayersAlreadyCommuned.Find(playerRef) >= 0
	If !alreadyCommuned && FFZ10_Light_AlreadyCommunedKeyword != None
		alreadyCommuned = playerRef.HasKeyword(FFZ10_Light_AlreadyCommunedKeyword)
	EndIf
	If alreadyCommuned
		Return
	EndIf

	Bool canCommune = PlayersCanCommune == None || PlayersCanCommune.Find(playerRef) >= 0
	If !canCommune && FFZ10_Light_CanCommuneKeyword != None
		canCommune = playerRef.HasKeyword(FFZ10_Light_CanCommuneKeyword)
	EndIf
	If !canCommune
		Return
	EndIf

	If PlayersCanCommune != None && PlayersCanCommune.Find(playerRef) >= 0
		PlayersCanCommune.RemoveRef(playerRef)
	EndIf
	If PlayersAlreadyCommuned != None && PlayersAlreadyCommuned.Find(playerRef) < 0
		PlayersAlreadyCommuned.AddRef(playerRef)
	EndIf
	If FFZ10_Light_CanCommuneKeyword != None
		playerRef.RemoveKeyword(FFZ10_Light_CanCommuneKeyword)
	EndIf
	If FFZ10_Light_AlreadyCommunedKeyword != None
		playerRef.AddKeyword(FFZ10_Light_AlreadyCommunedKeyword)
	EndIf
	If crXPBonusSpell != None
		playerRef.AddSpell(crXPBonusSpell, False)
	EndIf
	If FFZ10_Light_WiseMothmanFaction != None
		playerRef.AddToFaction(FFZ10_Light_WiseMothmanFaction)
	EndIf
	If FFZ10_Light_MothmanFriend != None
		playerRef.SetValue(FFZ10_Light_MothmanFriend, 1.0)
	EndIf

	Actor mothmanRef = GetActorReference()
	If mothmanRef != None
		If FFZ10_Light_WiseMothmanFaction != None
			mothmanRef.AddToFaction(FFZ10_Light_WiseMothmanFaction)
		EndIf
		If Mothman_Blessing != None
			mothmanRef.PlayIdle(Mothman_Blessing)
		EndIf
	EndIf

	owningQuest.SetObjectiveCompleted(20, True)
	owningQuest.SetStage(300)
	If FFZ10_Light_BlessingMessage != None
		FFZ10_Light_BlessingMessage.Show()
	EndIf
EndEvent
