Event OnAliasInit()
	ObjectReference playerRef = GetReference()
	If playerRef != None && MyCharGenSlotDoorKeyword != None
		ObjectReference roomDoor = playerRef.GetLinkedRef(MyCharGenSlotDoorKeyword)
		If roomDoor != None
			roomDoor.Lock(False)
		EndIf
	EndIf
EndEvent
