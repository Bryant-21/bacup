Function RestoreQuestCustom(Int restoreQuestStage)
	Actor player = Alias_Player.GetActorReference()
	If !player
		player = Game.GetPlayer()
		Alias_Player.ForceRefIfEmpty(player)
	EndIf
	If !player || !COMP_RQ_Fetch_KnownObjectsList
		Return
	EndIf

	CompanionScript companion = Alias_Companion.GetActorReference() as CompanionScript
	ObjectReference currentObject = Alias_Object.GetReference()
	If restoreQuestStage == StageToStore_COMP_AV_Player_COMP_RQ_FetchKnownObjectList_Index && currentObject && companion
		Int knownIndex = COMP_RQ_Fetch_KnownObjectsList.Find(currentObject.GetBaseObject())
		If knownIndex >= 0
			player.SetValue(companion.FetchKnownObjectList_Index_AV, knownIndex)
		EndIf
	EndIf

	ClearObjectAliases()
	If currentObject
		Alias_Object.ForceRefTo(currentObject)
		If restoreQuestStage >= RestoreStage_AddObjectToPlayer
			player.AddItem(currentObject, 1, True)
		EndIf
		Return
	EndIf
	If restoreQuestStage < RestoreStage_RestoreObject || !companion
		Return
	EndIf

	Int objectCount = COMP_RQ_Fetch_KnownObjectsList.GetSize()
	If objectCount <= 0
		Return
	EndIf
	Int objectIndex = Math.Floor(player.GetValue(companion.FetchKnownObjectList_Index_AV))
	If objectIndex < 0 || objectIndex >= objectCount
		objectIndex = 0
	EndIf
	Form objectBase = COMP_RQ_Fetch_KnownObjectsList.GetAt(objectIndex)
	If !objectBase
		objectIndex = 0
		objectBase = COMP_RQ_Fetch_KnownObjectsList.GetAt(objectIndex)
	EndIf
	If !objectBase
		Return
	EndIf

	ObjectReference restoredObject = player.PlaceAtMe(objectBase, 1, False, True)
	If restoredObject
		Alias_Object.ForceRefTo(restoredObject)
		If restoreQuestStage >= RestoreStage_AddObjectToPlayer
			player.AddItem(restoredObject, 1, True)
		EndIf
	EndIf
EndFunction

Function ClearObjectAliases()
	If ObjectAliasesToClear == None
		Return
	EndIf

	Int index = 0
	While index < ObjectAliasesToClear.Length
		If ObjectAliasesToClear[index]
			ObjectAliasesToClear[index].Clear()
		EndIf
		index += 1
	EndWhile
EndFunction
