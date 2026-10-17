Event OnLoad()
	ObjectReference selfRef = Self.GetReference()
	If selfRef && selfRef.IsDisabled()
		selfRef.Enable()
	EndIf
EndEvent
