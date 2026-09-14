Function PrepareHotdogs()
	Int hotdogIndex = 0
	While hotdogIndex < Hotdogs.GetCount()
		ObjectReference hotdogRef = Hotdogs.GetAt(hotdogIndex)
		If hotdogRef != None
			If hotdogIndex < NumOfStartingHotdogs
				hotdogRef.EnableNoWait()
			Else
				hotdogRef.DisableNoWait()
			EndIf
		EndIf
		hotdogIndex += 1
	EndWhile
EndFunction

Function RotateHotdog(ObjectReference akEatenHotdog)
	If akEatenHotdog != None
		akEatenHotdog.DisableNoWait()
	EndIf
	Utility.Wait(Utility.RandomFloat(HotdogEnableTimeMin, HotdogEnableTimeMax))

	Int hotdogIndex = 0
	While hotdogIndex < Hotdogs.GetCount()
		ObjectReference hotdogRef = Hotdogs.GetAt(hotdogIndex)
		If hotdogRef != None && hotdogRef.IsDisabled() && hotdogRef != akEatenHotdog
			hotdogRef.EnableNoWait()
			Return
		EndIf
		hotdogIndex += 1
	EndWhile

	If akEatenHotdog != None
		akEatenHotdog.EnableNoWait()
	EndIf
EndFunction
