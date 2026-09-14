Event OnTriggerEnter(ObjectReference akActionRef)
	If OnTriggerEnterLock || akActionRef == None
		Return
	EndIf
	OnTriggerEnterLock = True
	If akActionRef.HasKeyword(MTNM04_Undesirable_Keyword)
		UndesirablesInRoom.AddRef(akActionRef)
		akActionRef.AddKeyword(MTNM04_UndesirableTriggered_Keyword)
	EndIf
	OnTriggerEnterLock = False
EndEvent

Event OnTriggerLeave(ObjectReference akActionRef)
	If OnTriggerLeaveLock || akActionRef == None
		Return
	EndIf
	OnTriggerLeaveLock = True
	UndesirablesInRoom.RemoveRef(akActionRef)
	akActionRef.RemoveKeyword(MTNM04_UndesirableTriggered_Keyword)
	OnTriggerLeaveLock = False
EndEvent
