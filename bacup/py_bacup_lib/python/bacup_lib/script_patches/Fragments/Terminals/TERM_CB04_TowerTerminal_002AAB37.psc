Function Fragment_Terminal_01(ObjectReference akTerminalRef)
	Quest mayorQuest = Game.GetFormFromFile(0x002A93F5, "SeventySix.esm") as Quest
	If mayorQuest != None
		Int currentStage = mayorQuest.GetCurrentStageID()
		If currentStage >= 810 && currentStage < 900
			mayorQuest.SetStage(900)
		EndIf
	EndIf
EndFunction
