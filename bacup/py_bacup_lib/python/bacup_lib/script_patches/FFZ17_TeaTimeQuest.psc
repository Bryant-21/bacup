Event OnQuestShutdown()
	If QuestNextAvailTime == None || QuestCooldown == None
		Return
	EndIf
	; FO76 documents QuestCooldown in minutes; MinToExcelConst converts minutes to days, the unit of FO4 game time.
	QuestNextAvailTime.SetValue(Utility.GetCurrentGameTime() + QuestCooldown.GetValue() * MinToExcelConst)
EndEvent
