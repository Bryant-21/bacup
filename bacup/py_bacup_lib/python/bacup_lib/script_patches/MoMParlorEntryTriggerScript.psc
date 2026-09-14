Event OnTriggerEnter(ObjectReference akActionRef)
	Actor playerRef = akActionRef as Actor
	If playerRef != Game.GetPlayer() || MoMMaster == None || !MoMMaster.IsRunning()
		Return
	EndIf
	If MoM00 == None || MoM00.GetStage() < CONST_MoM00_QuestCompleted || playerRef.GetCurrentLocation() != RiversideManorLocation
		Return
	EndIf
	StartInitiateQuest(playerRef)
	If playerRef.GetValue(MoMParlorEntryLinePlayed) > 0.0 || MomParlorEntryTopics == None || MomParlorEntryTopics.Length == 0
		Return
	EndIf
	Int rankIndex = playerRef.GetValue(MoMRank) as Int
	If rankIndex < 0
		rankIndex = 0
	ElseIf rankIndex >= MomParlorEntryTopics.Length
		rankIndex = MomParlorEntryTopics.Length - 1
	EndIf
	playerRef.SetValue(MoMParlorEntryLinePlayed, 1.0)
	MoMCryptosVoiceF_VOICEONLY.Say(MomParlorEntryTopics[rankIndex], None, False, playerRef)
EndEvent

Function StartInitiateQuest(Actor playerRef)
	MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
	If masterScript == None || masterScript.MoMQuestList == None || masterScript.MoMQuestList.Length <= 2
		Return
	EndIf

	Quest initiateQuest = masterScript.MoMQuestList[2].MoMQuest
	If initiateQuest == None || initiateQuest.IsCompleted()
		Return
	EndIf
	If !initiateQuest.IsRunning()
		Keyword startKeyword = masterScript.MoMQuestList[2].MoMQuestKeyword
		If startKeyword != None
			startKeyword.SendStoryEventAndWait(None, playerRef)
		EndIf
	EndIf
	If initiateQuest.IsRunning() && !initiateQuest.IsStageDone(10)
		initiateQuest.SetStage(10)
	EndIf
EndFunction
