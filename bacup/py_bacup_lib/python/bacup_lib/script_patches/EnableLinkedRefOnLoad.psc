Event OnLoad()
	ObjectReference linked = Self.GetLinkedRef(None)
	If linked && linked.IsDisabled()
		linked.Enable(False)
	EndIf
EndEvent
