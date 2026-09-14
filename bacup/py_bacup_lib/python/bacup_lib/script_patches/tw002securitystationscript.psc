Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
	If auiMenuItemID == MenuID && akTerminalRef != None && akTerminalRef.HasKeyword(TW002Terminal)
		Self.SendCustomEvent("TW002GotTape", None)
	EndIf
EndEvent
