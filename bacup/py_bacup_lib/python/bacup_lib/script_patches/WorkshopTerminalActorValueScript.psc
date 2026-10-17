Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
	If auiMenuItemID < 0 || auiMenuItemID >= ActorValueList.Length
		Return
	EndIf
	ActorValue selectedValue = ActorValueList[auiMenuItemID]
	If selectedValue == None
		Return
	EndIf
	ObjectReference targetRef = akTerminalRef.GetLinkedRef(LinkTerminalKeyword)
	If targetRef == None
		Return
	EndIf
	; No FO76 terminal on this script binds ValueToSet (the pex default is 1), and
	; every one is a one-of-N output selector: Collectron (29), ArmCo and native ammo
	; factory (12), native scavenge (13) and food (7) factories, old Raider terminal (3).
	; The grouping must match mode_slices in attach_fo76_camp_collectors.rs.
	Float newValue = ValueToSet as Float
	If newValue == 0.0
		newValue = 1.0
	EndIf
	Int groupFirst = 0
	Int groupLast = ActorValueList.Length - 1
	; The Collectron terminals list every station's modes in one fixed order. The modes
	; of one station are one-of-N: Scavenger 0-1, Raider 2-4, Comrade 5-6. Only siblings
	; are cleared: a pick for another station's mode must not zero this station's resource.
	If ActorValueList.Length == 29
		If auiMenuItemID <= 1
			groupLast = 1
		ElseIf auiMenuItemID <= 4
			groupFirst = 2
			groupLast = 4
		ElseIf auiMenuItemID <= 6
			groupFirst = 5
			groupLast = 6
		Else
			groupFirst = auiMenuItemID
			groupLast = auiMenuItemID
		EndIf
	ElseIf ActorValueList.Length > 16
		groupFirst = auiMenuItemID
		groupLast = auiMenuItemID
	EndIf
	Int siblingIndex = groupFirst
	While siblingIndex <= groupLast
		If siblingIndex != auiMenuItemID && ActorValueList[siblingIndex] != None
			targetRef.SetValue(ActorValueList[siblingIndex], 0.0)
		EndIf
		siblingIndex += 1
	EndWhile
	targetRef.SetValue(selectedValue, newValue)
EndEvent
