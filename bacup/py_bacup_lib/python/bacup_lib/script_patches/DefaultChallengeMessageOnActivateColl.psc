Event OnAliasInit()
	Int index = 0
	While index < GetCount()
		ObjectReference collectionRef = GetAt(index)
		If collectionRef != None
			If BlockNormalActivationOnInit
				collectionRef.BlockActivation(True, True)
			EndIf
			RegisterForRemoteEvent(collectionRef, "OnActivate")
		EndIf
		index += 1
	EndWhile
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	Quest owningQuest = GetOwningQuest()
	Actor playerRef = PlayerAlias.GetReference() as Actor
	If akActionRef != playerRef || owningQuest == None || MessageToShow == None
		Return
	EndIf
	If (PreReqStage >= 0 && !owningQuest.IsStageDone(PreReqStage)) || (TurnOffStageSingle >= 0 && owningQuest.IsStageDone(TurnOffStageSingle))
		Return
	EndIf
	Int button = MessageToShow.Show()
	Int index = 0
	While index < MessageChallengeData.Length
		MessageChallengeDatum datum = MessageChallengeData[index]
		If datum.ButtonToCheck == button
			Bool passed = True
			If datum.Item_ToCheck != None
				passed = playerRef.GetItemCount(datum.Item_ToCheck) >= datum.ItemCountNeeded
			ElseIf datum.SPECIAL_ToCheck != None && datum.SPECIAL_Challenge_Global != None
				passed = playerRef.GetValue(datum.SPECIAL_ToCheck) >= datum.SPECIAL_Challenge_Global.GetValue()
			EndIf
			If passed && datum.StageToSet_OnPASS >= 0
				owningQuest.SetStage(datum.StageToSet_OnPASS)
			ElseIf !passed && datum.StageToSet_OnFAIL >= 0
				owningQuest.SetStage(datum.StageToSet_OnFAIL)
			EndIf
			If passed && datum.PassMessage != None
				datum.PassMessage.Show()
			ElseIf !passed && datum.FailMessage != None
				datum.FailMessage.Show()
			EndIf
			Return
		EndIf
		index += 1
	EndWhile
EndEvent
