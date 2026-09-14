Event OnQuestInit()
	UpdatePlayerVIPStatus(True)
EndEvent

Event OnQuestShutdown()
	UpdatePlayerVIPStatus(False)
	PlayerVIPTriggerCache = None
EndEvent

Function UpdatePlayerVIPStatus(Bool addPlayer)
	Actor player = Game.GetPlayer()
	If player == None
		Return
	EndIf

	If addPlayer
		If ActorGroupTriggers == None
			Return
		EndIf

		PlayerVIPTriggerCache = new ObjectReference[0]
		Int index = 0
		While index < ActorGroupTriggers.GetCount()
			ObjectReference triggerReference = ActorGroupTriggers.GetAt(index)
			DefaultTriggerRespawnActorGroup trigger = triggerReference as DefaultTriggerRespawnActorGroup
			If trigger
				trigger.AddPlayerAsVIP(player)
				PlayerVIPTriggerCache.Add(triggerReference)
			EndIf
			index += 1
		EndWhile
	ElseIf PlayerVIPTriggerCache != None
		Int index = 0
		While index < PlayerVIPTriggerCache.Length
			DefaultTriggerRespawnActorGroup trigger = PlayerVIPTriggerCache[index] as DefaultTriggerRespawnActorGroup
			If trigger
				trigger.RemovePlayerAsVIP(player)
			EndIf
			index += 1
		EndWhile
	EndIf
EndFunction
