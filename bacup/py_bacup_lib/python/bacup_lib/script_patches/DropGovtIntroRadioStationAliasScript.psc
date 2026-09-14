Event OnAliasInit()
	ObjectReference stationRef = GetReference()
	Terminal stationTerminal = None
	If stationRef != None
		stationTerminal = stationRef.GetBaseObject() as Terminal
	EndIf
	If stationTerminal != None
		RegisterForRemoteEvent(stationTerminal, "OnMenuItemRun")
	EndIf
EndEvent

Event Terminal.OnMenuItemRun(Terminal akSender, Int auiMenuItemID, ObjectReference akTerminalRef)
	Quest owningQuest = GetOwningQuest()
	If owningQuest != None && akTerminalRef == GetReference() && owningQuest.IsStageDone(100) && !owningQuest.IsStageDone(200)
		owningQuest.SetStage(200)
	EndIf
EndEvent

Event OnAliasShutdown()
	ObjectReference stationRef = GetReference()
	Terminal stationTerminal = None
	If stationRef != None
		stationTerminal = stationRef.GetBaseObject() as Terminal
	EndIf
	If stationTerminal != None
		UnregisterForRemoteEvent(stationTerminal, "OnMenuItemRun")
	EndIf
EndEvent
