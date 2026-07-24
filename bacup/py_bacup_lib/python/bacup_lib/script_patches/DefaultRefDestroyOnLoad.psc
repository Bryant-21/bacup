Event OnCellLoad()
	If !DestroyOnFirstLoadOnly || hasLoaded
		Return
	EndIf
	hasLoaded = True
	Self.Delete()
EndEvent
