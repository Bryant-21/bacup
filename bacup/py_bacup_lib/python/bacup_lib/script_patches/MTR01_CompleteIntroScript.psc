Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
	; "Access the Air Purifier" (MTR01_Intro objective 70): the purifier menu item records the
	; intro as complete and closes "Where There's Smoke..." at its completion stage.
	If auiMenuItemID != iMenuID
		Return
	EndIf
	Actor playerRef = Game.GetPlayer()
	If MTR01_Intro_Complete
		playerRef.SetValue(MTR01_Intro_Complete, 1.0)
	EndIf
	MTR01_QuestScript intro = Game.GetFormFromFile(0x00056F63, "SeventySix.esm") as MTR01_QuestScript
	If intro && intro.IsRunning() && intro.IsStageDone(70) && !intro.IsStageDone(intro.iCompletionStage)
		intro.SetStage(intro.iCompletionStage)
	EndIf
EndEvent
