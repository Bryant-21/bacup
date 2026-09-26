Event OnAliasInit()
	ObjectReference toggledRef = GetReference()
	If toggledRef == None
		Return
	EndIf
	If EnabledOnQuestActive
		toggledRef.Enable(False)
	Else
		toggledRef.Disable(False)
	EndIf
EndEvent

Event OnAliasShutdown()
	; The FO76 docstring returns the reference to its initial state, which is the opposite of the active one.
	ObjectReference toggledRef = GetReference()
	If toggledRef == None
		Return
	EndIf
	If EnabledOnQuestActive
		toggledRef.Disable(False)
	Else
		toggledRef.Enable(False)
	EndIf
EndEvent
