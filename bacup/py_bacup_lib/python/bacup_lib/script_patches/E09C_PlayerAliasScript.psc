Event OnAliasInit()
	OwningQuest = GetOwningQuest()
	QS = OwningQuest as E09C_QuestScript
	MissHandy = None
	If Alias_FakeMissHandy != None
		MissHandy = Alias_FakeMissHandy.GetReference()
	EndIf
	RegisterInventoryFilters()
EndEvent

Event OnAliasShutdown()
	RemoveAllInventoryEventFilters()
EndEvent

Function RegisterInventoryFilters()
	RemoveAllInventoryEventFilters()
	AddWatchedFilter(DecorationForm)
	AddWatchedFilter(E09C_RobotPartsList)
	Int index = 0
	While RobotPartsStruct != None && index < RobotPartsStruct.Length
		If RobotPartsStruct[index] != None
			AddWatchedFilter(RobotPartsStruct[index].RobotPartForm)
		EndIf
		index += 1
	EndWhile
EndFunction

Function AddWatchedFilter(Form akFilter)
	If akFilter == None
		Return
	EndIf
	AddInventoryEventFilter(akFilter)
	; Collection handlers receive their members' inventory events, so the player needs the filter as well.
	; DefaultQuestRemovePlayersScript adds the player after this alias initializes.
	Actor player = Game.GetPlayer()
	If player != None
		player.AddInventoryEventFilter(akFilter)
	EndIf
	Int index = 0
	While index < GetCount()
		ObjectReference memberRef = GetAt(index)
		If memberRef != None && memberRef != player
			memberRef.AddInventoryEventFilter(akFilter)
		EndIf
		index += 1
	EndWhile
EndFunction

Bool Function IsEventItem(Form akItem)
	If akItem == None
		Return False
	EndIf
	If akItem == DecorationForm
		Return True
	EndIf
	If E09C_RobotPartsList != None && E09C_RobotPartsList.HasForm(akItem)
		Return True
	EndIf
	Return RobotPartsStruct != None && RobotPartsStruct.FindStruct("RobotPartForm", akItem) >= 0
EndFunction

Event OnItemAdded(ObjectReference akSenderRef, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
	If akSenderRef != Game.GetPlayer() || !IsEventItem(akBaseItem)
		Return
	EndIf
	; QO_Items marks carried event items as quest objects; DefaultQuestRemovePlayersScript removes them at shutdown.
	If akItemReference != None && QO_Items != None && QO_Items.Find(akItemReference) < 0
		QO_Items.AddRef(akItemReference)
	EndIf
	If QS != None
		QS.PublishPhaseVariables()
	EndIf
EndEvent

Event OnItemRemoved(ObjectReference akSenderRef, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akDestContainer)
	If akSenderRef != Game.GetPlayer() || akDestContainer != None || RobotPartsStruct == None
		Return
	EndIf
	Int partIndex = RobotPartsStruct.FindStruct("RobotPartForm", akBaseItem)
	If partIndex < 0
		Return
	EndIf
	; A dropped part lands in the world; one handed to Miss Lovely is consumed by her trigger's RemoveItem.
	If akItemReference != None && !akItemReference.IsDeleted() && akItemReference.GetParentCell() != None
		Return
	EndIf
	If RobotPartsStruct[partIndex].RobotPartActorValue != None
		akSenderRef.SetValue(RobotPartsStruct[partIndex].RobotPartActorValue, 1.0)
	EndIf
	; Miss Lovely's trigger calls BuildHandy without waiting, possibly before this event raises the value.
	If QS != None
		QS.BuildHandy()
	EndIf
EndEvent

Event OnItemEquipped(ObjectReference akSenderRef, Form akBaseObject, ObjectReference akReference)
	If akSenderRef != Game.GetPlayer() || akBaseObject == None || QS == None
		Return
	EndIf
	Bool isFood = ObjectTypeFood != None && akBaseObject.HasKeyword(ObjectTypeFood)
	Bool isDrink = ObjectTypeDrink != None && akBaseObject.HasKeyword(ObjectTypeDrink)
	If !isFood && !isDrink
		Return
	EndIf
	Quest owner = GetOwningQuest()
	If owner == None || !owner.IsStageDone(iWeddingRequiredStage) || owner.IsStageDone(iWeddingTurnOffStage)
		Return
	EndIf
	If Alias_FakeMissHandy != None
		MissHandy = Alias_FakeMissHandy.GetReference()
	EndIf
	If MissHandy != None && akSenderRef.GetDistance(MissHandy) > fDistanceToWedding
		Return
	EndIf
	QS.AddWeddingMerriment(1)
EndEvent
