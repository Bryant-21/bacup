Event OnAliasInit()
	HivesLooted = 0
	RegisterHiveInventoryEvents()
EndEvent

Event OnAliasReset()
	HivesLooted = 0
	RegisterHiveInventoryEvents()
EndEvent

; OnItemRemoved below is dispatched by each member reference, and a reference only
; dispatches inventory events once a filter is registered on it. RefCollectionAlias
; has no filter API of its own, so arm every member individually or the hive-looted
; count never advances and the quest cannot progress.
Function RegisterHiveInventoryEvents()
	Int index = GetCount() - 1
	While index >= 0
		ObjectReference hiveRef = GetAt(index)
		If hiveRef != None
			hiveRef.AddInventoryEventFilter(None)
		EndIf
		index -= 1
	EndWhile
EndFunction

Event OnItemRemoved(ObjectReference akSenderRef, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
	If akSenderRef == None || akBaseItem == None || aiItemCount <= 0 || Alias_Player == None || akDestContainer != Alias_Player.GetReference()
		Return
	EndIf

	Quest owningQuest = GetOwningQuest()
	If owningQuest == None || owningQuest.IsStageDone(StageToSetToDisableQT) || Find(akSenderRef) < 0
		Return
	EndIf

	If akSenderRef.GetItemCount(akBaseItem) > 0
		Return
	EndIf

	Int hiveCount = GetCount() + HivesLooted
	RemoveRef(akSenderRef)
	HivesLooted += 1
	If hiveCount > 0 && (HivesLooted as Float) / hiveCount >= PercentHivesLootedToDisableQT
		owningQuest.SetStage(StageToSetToDisableQT)
	EndIf
EndEvent
