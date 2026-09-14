Event OnInit()
	If RQData == None
		Return
	EndIf

	Int index = 0
	While index < RQData.Length
		If RQData[index].BaseQuest
			UnregisterForRemoteEvent(RQData[index].BaseQuest, "OnStageSet")
			RegisterForRemoteEvent(RQData[index].BaseQuest, "OnStageSet")
		EndIf
		index += 1
	EndWhile
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
	Actor player = Game.GetPlayer()
	If player == None || RQData == None
		Return
	EndIf

	Int index = 0
	While index < RQData.Length
		RQDatum currentDatum = RQData[index]
		If currentDatum.BaseQuest == akSender && currentDatum.Stage == auiStageID
			ApplyRadiantQuestStageDatum(player, currentDatum)
		EndIf
		index += 1
	EndWhile
EndEvent

Function ApplyRadiantQuestStageDatum(Actor player, RQDatum currentDatum)
	If currentDatum.PlayerActorValueToSet == None
		Return
	EndIf
	If GetValue(COMP_QuestCount) < currentDatum.RequiredQuestCount
		Return
	EndIf
	If currentDatum.IgnoreOnPlayerActorValue && player.GetValue(currentDatum.IgnoreOnPlayerActorValue) == currentDatum.IgnoreOnValue
		Return
	EndIf

	player.SetValue(currentDatum.PlayerActorValueToSet, currentDatum.ValueToSet)
EndFunction
