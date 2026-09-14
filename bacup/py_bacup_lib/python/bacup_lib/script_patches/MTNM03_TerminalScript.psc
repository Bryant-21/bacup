Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
	Actor playerRef = Game.GetPlayer()
	If playerRef == None || MTNM03_Misc_Quest_Keyword == None
		Return
	EndIf
	If MTNM03_Misc_QuestActive_Keyword == None || !playerRef.HasKeyword(MTNM03_Misc_QuestActive_Keyword)
		MTNM03_Misc_Quest_Keyword.SendStoryEventAndWait(playerRef.GetCurrentLocation(), playerRef, akTerminalRef)
	EndIf
EndEvent
