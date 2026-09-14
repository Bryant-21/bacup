Event OnTriggerEnter(ObjectReference akActionRef)
	If OnTriggerEnterLock || akActionRef == None
		Return
	EndIf
	OnTriggerEnterLock = True
	If MTNM04_Undesirable_Keyword != None
		akActionRef.AddKeyword(MTNM04_Undesirable_Keyword)
	EndIf
	If AllAttackers != None
		AllAttackers.AddRef(akActionRef)
	EndIf
	OnTriggerEnterLock = False
EndEvent
