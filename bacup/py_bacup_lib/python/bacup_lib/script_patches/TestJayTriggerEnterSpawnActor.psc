Event OnTriggerEnter(ObjectReference akActionRef)
	If akActionRef != Game.GetPlayer() || SpawnActorBase == None || SpawnActorCount <= 0
		Return
	EndIf
	ObjectReference spawnOrigin = SpawnPointParent
	If spawnOrigin == None
		spawnOrigin = GetLinkedRef(SpawnPointParentKeyword)
	EndIf
	If spawnOrigin != None
		spawnOrigin.PlaceAtMe(SpawnActorBase, SpawnActorCount, False, False, True)
	EndIf
EndEvent
