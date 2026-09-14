Event OnPowerOn(ObjectReference akPowerGenerator)
	If busy
		Return
	EndIf
	busy = True
	ObjectReference[] linkedChildren = GetLinkedRefChildren(None)
	If linkedChildren.Length > 0
		Int selectedIndex = Utility.RandomInt(0, linkedChildren.Length - 1)
		If linkedChildren[selectedIndex] != None
			linkedChildren[selectedIndex].Activate(Self)
		EndIf
	EndIf
	busy = False
EndEvent
