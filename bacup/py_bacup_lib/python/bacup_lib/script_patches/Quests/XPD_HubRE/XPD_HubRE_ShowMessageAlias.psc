Bool Function ChallengePassed(MessageChallengeDatum challenge, Actor player)
	If challenge == None || player == None
		Return False
	EndIf
	If challenge.Item_ToCheck != None
		Return player.GetItemCount(challenge.Item_ToCheck) >= challenge.ItemCountNeeded
	EndIf
	If challenge.SPECIAL_ToCheck != None && challenge.SPECIAL_Challenge_Global != None
		Return player.GetValue(challenge.SPECIAL_ToCheck) >= challenge.SPECIAL_Challenge_Global.GetValue()
	EndIf
	Return True
EndFunction

Function ConsumeRepairItems(MessageChallengeDatum challenge, Actor player)
	If challenge != None && player != None && challenge.Item_ToCheck != None && challenge.ItemCountNeeded > 0
		player.RemoveItem(challenge.Item_ToCheck, challenge.ItemCountNeeded, True)
	EndIf
EndFunction

Int Function ShowChallengeMessage(Message challengeMessage)
	Return challengeMessage.Show()
EndFunction

Event OnAliasInit()
	If BlockNormalActivationOnInit && GetReference() != None
		GetReference().BlockActivation(True)
	EndIf
EndEvent

Event OnActivate(ObjectReference akActionRef)
	Actor player = PlayerAlias.GetActorReference()
	Quest owner = GetOwningQuest()
	If akActionRef != player || player == None || owner == None || !owner.IsRunning()
		Return
	EndIf
	If PreReqStage > 0 && !owner.IsStageDone(PreReqStage)
		Return
	EndIf
	If TurnOffStageSingle > 0 && owner.IsStageDone(TurnOffStageSingle)
		Return
	EndIf
	If MessageToShow == None || MessageChallengeData == None
		Return
	EndIf

	Bool showMenu = True
	While showMenu
		showMenu = False
		Int selectedButton = ShowChallengeMessage(MessageToShow)
		Int index = 0
		While index < MessageChallengeData.Length
			MessageChallengeDatum challenge = MessageChallengeData[index]
			If challenge != None && challenge.ButtonToCheck == selectedButton
				If ChallengePassed(challenge, player)
					ConsumeRepairItems(challenge, player)
					If challenge.PassMessage != None
						challenge.PassMessage.Show()
					EndIf
					If challenge.StageToSet_OnPASS > 0 && !owner.IsStageDone(challenge.StageToSet_OnPASS)
						owner.SetStage(challenge.StageToSet_OnPASS)
					EndIf
				Else
					If challenge.FailMessage != None
						challenge.FailMessage.Show()
					EndIf
					If challenge.StageToSet_OnFAIL > 0 && !owner.IsStageDone(challenge.StageToSet_OnFAIL)
						owner.SetStage(challenge.StageToSet_OnFAIL)
					EndIf
					If challenge.ReturnToTop
						showMenu = True
					EndIf
				EndIf
				If !showMenu
					Return
				EndIf
			EndIf
			index += 1
		EndWhile
	EndWhile
EndEvent
