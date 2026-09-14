Function Fragment_Terminal_01(ObjectReference akTerminalRef)
	Quest mayorQuest = Game.GetFormFromFile(0x002A93F5, "SeventySix.esm") as Quest
	If mayorQuest != None
		Int currentStage = mayorQuest.GetCurrentStageID()
		If currentStage >= 100 && currentStage < 200
			mayorQuest.SetStage(200)
		EndIf
	EndIf
EndFunction
