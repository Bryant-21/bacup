Function AddPlayerAsVIP(Actor PlayerToAdd)
	; TODO
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
