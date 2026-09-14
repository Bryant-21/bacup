Event OnLoad()
	AddInventoryEventFilter(BoSTechnicalDocument)
	TryStartBoSZ01()
EndEvent

Event OnUnload()
	RemoveInventoryEventFilter(BoSTechnicalDocument)
	CancelTimer(1)
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
	If akBaseItem == BoSTechnicalDocument && aiItemCount > 0
		TryStartBoSZ01()
	EndIf
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == 1
		TryStartBoSZ01()
	EndIf
EndEvent

Function TryStartBoSZ01()
	If BoSZ01 == None || BoSTechnicalDocument == None || BoSZ01.IsRunning()
		Return
	EndIf
	If GetItemCount(BoSTechnicalDocument) <= 0 || BoSZ01_QuestStartKeyword == None
		Return
	EndIf

	CancelTimer(1)
	BoSZ01_QuestStartKeyword.SendStoryEventAndWait(akRef1 = Self)
	If !BoSZ01.IsRunning() && GetItemCount(BoSTechnicalDocument) > 0
		StartTimer(1.0, 1)
	EndIf
EndFunction
