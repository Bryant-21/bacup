Function Fragment_Terminal_01(ObjectReference akTerminalRef)
	CB04_QuestScript mayorQuest = Game.GetFormFromFile(0x002A93F5, "SeventySix.esm") as CB04_QuestScript
	If mayorQuest != None
		Int currentStage = mayorQuest.GetCurrentStageID()
		If currentStage >= 300 && currentStage < 500
			mayorQuest.AddClue(3)
		EndIf
	EndIf
EndFunction
