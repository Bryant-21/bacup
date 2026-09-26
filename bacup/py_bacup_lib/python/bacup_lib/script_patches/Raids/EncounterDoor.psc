; The raid controller calls Unseal() on a wipe reset. OnQuestShutdown cannot unseal: the
; volume alias is already empty by then.
Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == 100
		Seal()
	ElseIf auiStageID == 9000 || auiStageID == 9999
		Unseal()
	EndIf
EndEvent

Function Seal()
	ObjectReference volume = EncounterVolume()
	If volume == None
		Return
	EndIf
	ObjectReference firstDoor = volume.GetLinkedRef(EC_Encounter_Doors)
	PullPlayerInside(volume, firstDoor)
	ObjectReference doorRef = firstDoor
	Int guard = 0
	While doorRef != None && guard < 16
		If doorRef.GetOpenState() != DoorClosedState
			doorRef.SetOpen(False)
		EndIf
		doorRef.SetLockLevel(254)
		doorRef.Lock(True)
		doorRef = doorRef.GetLinkedRef(EC_Encounter_Doors)
		If doorRef == firstDoor
			doorRef = None
		EndIf
		guard += 1
	EndWhile
EndFunction

Function Unseal()
	ObjectReference volume = EncounterVolume()
	If volume == None
		Return
	EndIf
	ObjectReference firstDoor = volume.GetLinkedRef(EC_Encounter_Doors)
	ObjectReference doorRef = firstDoor
	Int guard = 0
	While doorRef != None && guard < 16
		doorRef.Lock(False)
		doorRef.SetOpen(True)
		doorRef = doorRef.GetLinkedRef(EC_Encounter_Doors)
		If doorRef == firstDoor
			doorRef = None
		EndIf
		guard += 1
	EndWhile
EndFunction

ObjectReference Function EncounterVolume()
	If EC_Encounter_Volume == None || EC_Encounter_Doors == None
		Return None
	EndIf
	Return EC_Encounter_Volume.GetReference()
EndFunction

; A player farther from the volume centre than the entrance doorRef is on the approach side and would
; be locked out; a false positive only hops a player who is already inside onto an inside marker.
Function PullPlayerInside(ObjectReference akVolume, ObjectReference akFirstDoor)
	If EC_Teleport_Markers == None || akFirstDoor == None
		Return
	EndIf
	ObjectReference marker = akVolume.GetLinkedRef(EC_Teleport_Markers)
	Actor player = Game.GetPlayer()
	If marker == None || player.GetParentCell() != akVolume.GetParentCell()
		Return
	EndIf
	If player.GetDistance(akVolume) > akFirstDoor.GetDistance(akVolume)
		player.MoveTo(marker)
	EndIf
EndFunction
