Function Fragment_Terminal_01(ObjectReference akTerminalRef)
	Quest bosz04 = Game.GetFormFromFile(0x00065DFE, "SeventySix.esm") as Quest
	Debug.Trace("[B21 BoSZ04] Power terminal selected ref=" + akTerminalRef as String + " quest=" + bosz04 as String, 0)
	If bosz04 != None && bosz04.IsRunning()
		Int currentStage = bosz04.GetCurrentStageID()
		Debug.Trace("[B21 BoSZ04] Power terminal current stage=" + currentStage as String, 0)
		If currentStage >= 80 && currentStage < 95
			bosz04.SetStage(95)
			Debug.Trace("[B21 BoSZ04] Power terminal stage result=" + bosz04.GetStage() as String, 0)
		EndIf
	Else
		Debug.Trace("[B21 BoSZ04] Power terminal quest missing or not running", 0)
	EndIf
EndFunction
