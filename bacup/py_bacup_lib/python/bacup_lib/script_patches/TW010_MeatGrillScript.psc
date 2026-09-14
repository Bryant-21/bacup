Event OnActivate(ObjectReference akActionRef)
	Actor playerRef = Game.GetPlayer()
	Quest owningQuest = GetOwningQuest()
	If playerRef == None || akActionRef != playerRef || owningQuest == None
		Return
	EndIf
	If owningQuest.IsStageDone(StageToSetMeat)
		Return
	EndIf
	If !owningQuest.IsStageDone(PrereqStageMeat) || RadstagMeatRaw == None || playerRef.GetItemCount(RadstagMeatRaw) < RadstagMeatCount
		If TW010GrillNotReadyMsg != None
			TW010GrillNotReadyMsg.Show()
		EndIf
		Return
	EndIf

	Bool turnInYaoGuai = YaoGuaiMeatRaw != None && playerRef.GetItemCount(YaoGuaiMeatRaw) >= YaoGuaiMeatCount
	Bool turnInDeathclaw = DeathclawMeatRaw != None && playerRef.GetItemCount(DeathclawMeatRaw) >= DeathclawMeatCount

	playerRef.RemoveItem(RadstagMeatRaw, RadstagMeatCount, True)
	If turnInYaoGuai
		playerRef.RemoveItem(YaoGuaiMeatRaw, YaoGuaiMeatCount, True)
	EndIf
	If turnInDeathclaw
		playerRef.RemoveItem(DeathclawMeatRaw, DeathclawMeatCount, True)
	EndIf

	owningQuest.SetStage(StageToSetMeat)
	If turnInYaoGuai
		owningQuest.SetStage(YaoGuaiStage)
	EndIf
	If turnInDeathclaw
		owningQuest.SetStage(DeathclawStage)
	EndIf
EndEvent
