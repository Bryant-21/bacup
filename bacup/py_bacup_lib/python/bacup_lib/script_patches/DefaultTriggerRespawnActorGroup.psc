; TODO
; Deferred: no FO4 replacement for the nearby-players message receiver list.

Function ShowTimeStamps()
	; FO4 Debug.Trace takes (text, severity) only; the FO76 trace-channel argument
	; is dropped and TraceChannel is folded into the message text instead.
	debug.Trace(TraceChannel + ": " + Self as String + " ShowTimeStamps()| ShowActorTimeStampList :: CurrentRealTime == " + utility.GetCurrentRealTime() as String + " Next OccupiedRespawnTime == " + (LastTriggeredTime + fOccupiedResetTimeMinutes * 60.0) as String, 0)
	Int index = 0
	Int Count = TrackedPlayers.Length
	If Count > 0
		While index < Count
			debug.Trace(TraceChannel + ": " + Self as String + " ShowTimeStamps()| Player " + TrackedPlayers[index].TrackedPlayerID as String + " Next Valid TriggerTime == " + (TrackedPlayers[index].TimeStamp + fResetTimeMinutes * 60.0) as String, 0)
			index = index + 1
		EndWhile
	Else
		debug.Trace(TraceChannel + ": " + Self as String + " ShowTimeStamps()| No Actors currently in Timestamp List", 0)
	EndIf
EndFunction

Function ShowSpawnNotification(actor[] LinkedActors)
	; FO76 sent this message to the set of players inside 6000 units
	; (Message.Show(Actor[] receivers, ...)). FO4's Message.Show has no receiver
	; list and always targets the single player, so the radius filter is applied
	; directly and the extra FO76 argument slots are dropped.
	Actor thePlayer = Game.GetPlayer()
	If InCellRespawnMsg == None || thePlayer == None || Self.GetDistance(thePlayer) > 6000.0
		Return
	EndIf
	InCellRespawnMsg.Show(LinkedActors.Length as Float)
EndFunction

Function AddPlayerAsVIP(Actor PlayerToAdd)
	If PlayerToAdd == None || VIP_Players.Find(PlayerToAdd) >= 0
		Return
	EndIf
	VIP_Players.Add(PlayerToAdd)
EndFunction

Function RemovePlayerAsVIP(Actor PlayerToRemove)
	Int index = VIP_Players.Find(PlayerToRemove)
	If index < 0
		Return
	EndIf
	VIP_Players.Remove(index)
EndFunction
