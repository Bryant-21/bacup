Event OnLoad()
	Int index = 0
	While index < LinkedRefs.Length
		If LinkedRefs[index] != None
			LinkedRefs[index].Activate(Self)
		EndIf
		index += 1
	EndWhile
	If DoOnce
		LinkedRefs.Clear()
	EndIf
EndEvent
