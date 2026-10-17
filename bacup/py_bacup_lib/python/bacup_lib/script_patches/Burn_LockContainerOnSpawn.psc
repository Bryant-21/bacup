Event OnAliasInit()
	ObjectReference containerRef = GetReference()
	If containerRef != None
		containerRef.Lock(True)
		containerRef.SetLockLevel(LockLevel)
	EndIf
EndEvent
