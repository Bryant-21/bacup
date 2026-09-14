Event OnInit()
	akPlayer = Game.GetPlayer()
	ReconcileCollectableProgress()
EndEvent

Function ReconcileCollectableProgress()
	If akPlayer == None
		akPlayer = Game.GetPlayer()
	EndIf
	If akPlayer == None || !IsRunning() || !IsStageDone(BodhiInitialConversationStage)
		Return
	EndIf

	Int batchIndex = akPlayer.GetValue(Burn_SQ04_TotalBatchesCollected) as Int
	PlayerCollectableProgress = batchIndex
	If batchIndex < 0 || batchIndex >= CollectablesNeededPerBatch.Length
		rewardReady = False
		finalHandInReady = False
		Return
	EndIf

	Float heldIntel = akPlayer.GetValue(Burn_SQ04_CollectablesNotHandedIn)
	rewardReady = heldIntel >= CollectablesNeededPerBatch[batchIndex]
	finalHandInReady = rewardReady && batchIndex == CollectablesNeededPerBatch.Length - 1
	If rewardReady && !IsStageDone(BatchCollectedStages[batchIndex])
		SetStage(BatchCollectedStages[batchIndex])
	EndIf
EndFunction

Bool Function HandInReadyBatch()
	If akPlayer == None
		akPlayer = Game.GetPlayer()
	EndIf
	If akPlayer == None || !IsRunning() || !IsStageDone(BodhiInitialConversationStage)
		Return False
	EndIf

	Int batchIndex = akPlayer.GetValue(Burn_SQ04_TotalBatchesCollected) as Int
	If batchIndex < 0 || batchIndex >= CollectablesNeededPerBatch.Length
		Return False
	EndIf
	If IsStageDone(BatchRewardStages[batchIndex])
		ReconcileCollectableProgress()
		Return False
	EndIf

	Int neededIntel = CollectablesNeededPerBatch[batchIndex]
	Float heldIntel = akPlayer.GetValue(Burn_SQ04_CollectablesNotHandedIn)
	If heldIntel < neededIntel
		ReconcileCollectableProgress()
		Return False
	EndIf
	If !IsStageDone(BatchCollectedStages[batchIndex])
		SetStage(BatchCollectedStages[batchIndex])
	EndIf

	akPlayer.ModValue(Burn_SQ04_CollectablesNotHandedIn, 0 - neededIntel)
	akPlayer.SetValue(Burn_SQ04_TotalBatchesCollected, (batchIndex + 1) as Float)
	GrantBatchReward(batchIndex)
	SetStage(BatchRewardStages[batchIndex])
	ReconcileCollectableProgress()
	Return True
EndFunction

Function GrantBatchReward(Int aiBatchIndex)
	If akPlayer == None || RewardsList == None || aiBatchIndex < 0 || aiBatchIndex >= RewardsList.Length
		Return
	EndIf
	RewardItemLists rewardData = RewardsList[aiBatchIndex]
	If Caps001 != None && rewardData.CapsReward > 0
		akPlayer.AddItem(Caps001, rewardData.CapsReward, False)
	EndIf
	If rewardData.BonusItems != None
		akPlayer.AddItem(rewardData.BonusItems, 1, False)
	EndIf
	If rewardData.HolotapeReward != None
		akPlayer.AddItem(rewardData.HolotapeReward, 1, False)
	EndIf
EndFunction
