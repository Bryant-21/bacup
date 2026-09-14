Function DisableHiveMarker(Int aiHiveIndex)
	If aiHiveIndex >= 0 && aiHiveIndex < ChosenHiveQTs.Length && ChosenHiveQTs[aiHiveIndex] != None
		ChosenHiveQTs[aiHiveIndex].Clear()
	EndIf
EndFunction
