Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
	If auiMenuItemID < 0 || auiMenuItemID >= ActorValueList.Length
		Return
	EndIf
	ActorValue selectedValue = ActorValueList[auiMenuItemID]
	If selectedValue == None
		Return
	EndIf
	ObjectReference targetRef = akTerminalRef.GetLinkedRef(LinkTerminalKeyword)
	If targetRef != None
		targetRef.SetValue(selectedValue, ValueToSet as Float)
	EndIf
EndEvent
