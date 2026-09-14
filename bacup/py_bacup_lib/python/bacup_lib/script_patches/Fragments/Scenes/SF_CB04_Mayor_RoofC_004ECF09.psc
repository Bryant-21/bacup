Function Fragment_Phase_01_End()
	Quest mayorQuest = Game.GetFormFromFile(0x002A93F5, "SeventySix.esm") as Quest
	If mayorQuest != None && mayorQuest.IsCompleted()
		mayorQuest.Stop()
	EndIf
EndFunction
