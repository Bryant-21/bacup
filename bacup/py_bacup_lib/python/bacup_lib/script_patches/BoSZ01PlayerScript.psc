; @extends ReferenceAlias
; FO76 put this on the player's NPC_ record, which FO4 cannot carry. Tales'
; B21_TFA_qust_PlayerScripts holds it on a PlayerRef alias instead.
; BoSZ01 is repeatable while the player holds Technical Data. Its Story Manager node can still
; refuse a start (BoS01 not finished), so the pickup is re-offered on load instead of polled.
Event OnAliasInit()
	ResetTechnicalDataFilter()
	TryStartBoSZ01()
EndEvent

Event OnPlayerLoadGame()
	ResetTechnicalDataFilter()
	TryStartBoSZ01()
EndEvent

Event OnItemAdded(Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
	If akBaseItem == BoSTechnicalDocument && aiItemCount > 0
		TryStartBoSZ01()
	EndIf
EndEvent

Function ResetTechnicalDataFilter()
	RemoveAllInventoryEventFilters()
	If BoSTechnicalDocument != None
		AddInventoryEventFilter(BoSTechnicalDocument)
	EndIf
EndFunction

Function TryStartBoSZ01()
	Actor playerRef = GetActorReference()
	If playerRef == None || BoSZ01 == None || BoSTechnicalDocument == None || BoSZ01_QuestStartKeyword == None
		Return
	EndIf
	If BoSZ01.IsRunning() || playerRef.GetItemCount(BoSTechnicalDocument) <= 0
		Return
	EndIf
	BoSZ01_QuestStartKeyword.SendStoryEventAndWait(None, playerRef, playerRef)
EndFunction
