Event OnLoad()
	Self.RequestSwordVisibility(Game.GetPlayer())
EndEvent

Event OnActivate(ObjectReference akActionRef)
	Actor player = akActionRef as Actor
	If player != Game.GetPlayer() || !swordReadyForPickup
		Return
	EndIf

	MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
	If masterScript == None || masterScript.MoMQuestList == None || masterScript.MoMQuestList.Length <= 5
		Return
	EndIf

	Quest targetQuest = masterScript.MoMQuestList[5].MoMQuest
	MoM02BQuestScript questScript = targetQuest as MoM02BQuestScript
	If targetQuest == None || questScript == None || !targetQuest.IsRunning() || targetQuest.IsStageDone(50)
		Return
	EndIf

	swordReadyForPickup = False
	If player.GetItemCount(questScript.MoM02BHistoricSword) == 0
		player.AddItem(questScript.MoM02BHistoricSword, 1, True)
	EndIf
	SetSwordVisibility(False, True)
	targetQuest.SetStage(50)
EndEvent

Function RequestSwordVisibility(Actor akSendingPlayer)
	MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
	If akSendingPlayer == None || masterScript == None || masterScript.MoMQuestList == None || masterScript.MoMQuestList.Length <= 5
		swordReadyForPickup = False
		SetSwordVisibility(False, False)
		Return
	EndIf

	Quest targetQuest = masterScript.MoMQuestList[5].MoMQuest
	swordReadyForPickup = targetQuest != None && targetQuest.IsRunning() && targetQuest.IsStageDone(CONST_CHECKPOINT_MoM02B_LocateTargets) && !targetQuest.IsStageDone(50)
	SetSwordVisibility(swordReadyForPickup, False)
EndFunction
