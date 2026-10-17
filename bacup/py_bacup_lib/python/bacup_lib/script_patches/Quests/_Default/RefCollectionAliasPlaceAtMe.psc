Event OnAliasInit()
	Quest owner = Self.GetOwningQuest()
	If StageToPlace < 0
		PlaceAtAll()
	ElseIf owner
		If owner.IsStageDone(StageToPlace)
			PlaceAtAll()
		Else
			Self.RegisterForRemoteEvent(owner, "OnStageSet")
		EndIf
	EndIf
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
	If auiStageID == StageToPlace
		Self.UnregisterForRemoteEvent(akSender, "OnStageSet")
		PlaceAtAll()
	EndIf
EndEvent

Function PlaceAtAll()
	If !ItemToPlace
		Return
	EndIf
	ReferenceAlias fillRef = AliasToFill as ReferenceAlias
	RefCollectionAlias fillCollection = AliasToFill as RefCollectionAlias
	Int i = 0
	Int count = Self.GetCount()
	While i < count
		ObjectReference anchor = Self.GetAt(i)
		If anchor
			ObjectReference placed = anchor.PlaceAtMe(ItemToPlace, 1, False, False, True)
			If placed
				If fillCollection
					fillCollection.AddRef(placed)
				ElseIf fillRef
					fillRef.ForceRefTo(placed)
				EndIf
			EndIf
		EndIf
		i += 1
	EndWhile
EndFunction
