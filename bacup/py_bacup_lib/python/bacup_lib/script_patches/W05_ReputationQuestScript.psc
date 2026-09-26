; Each row names the reputation AV, the tier global that marks the faction's
; Hostile threshold (Rep_Tier_<Faction>_0_Hostile) and the stage the quest takes
; when the player falls to it. FO4 has no actor-value-changed event, so the
; comparison is polled while the quest runs.
Event OnQuestInit()
	EvaluateReputationTiers()
	StartTimer(5.0, 76050)
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID != 76050
		Return
	EndIf
	If !IsRunning()
		Return
	EndIf
	EvaluateReputationTiers()
	StartTimer(5.0, 76050)
EndEvent

Event OnQuestShutdown()
	CancelTimer(76050)
EndEvent

Function EvaluateReputationTiers()
	If !IsRunning() || ReputationTierStageData == None || Alias_Player == None
		Return
	EndIf
	Actor player = Alias_Player.GetActorReference()
	If player == None
		Return
	EndIf
	Int index = 0
	While index < ReputationTierStageData.Length
		ReputationTierStageDatum row = ReputationTierStageData[index]
		If row != None && row.Reputation_AV != None && row.ReputationTier_Global != None && !IsStageDone(row.StageToSet)
			If player.GetValue(row.Reputation_AV) <= row.ReputationTier_Global.GetValue()
				SetStage(row.StageToSet)
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction
