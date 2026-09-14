Function Fragment_End(ObjectReference akSpeakerRef)
	Quests:U01A_Brewing:MasterScript masterScript = D01A_Brewing_MasterQuest as Quests:U01A_Brewing:MasterScript
	If masterScript != None
		masterScript.ChooseAndStartDailyQuest()
	EndIf
EndFunction
