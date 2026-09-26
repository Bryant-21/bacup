Quest Function GetBoSZ04()
	Return Game.GetFormFromFile(0x00065DFE, "SeventySix.esm") as Quest
EndFunction

Function B21RunAutomatedTest(ObjectReference akTerminalRef, String asEntry)
	Quest bosz04 = GetBoSZ04()
	Debug.Trace("[B21 BoSZ04] VTU terminal " + asEntry + " selected ref=" + akTerminalRef as String + " quest=" + bosz04 as String, 0)
	If bosz04 == None || !bosz04.IsRunning()
		Debug.Trace("[B21 BoSZ04] VTU terminal quest missing or not running", 0)
		Return
	EndIf
	Int currentStage = bosz04.GetCurrentStageID()
	Debug.Trace("[B21 BoSZ04] VTU terminal current stage=" + currentStage as String, 0)
	; All three menu entries read "Execute automated test" and FO76 told them apart with
	; GetStageDone conditions that the conversion drops, so whichever entry survives for
	; the player has to run the step the quest is actually on.
	If currentStage >= 300 && currentStage < 350
		bosz04.SetStage(350)
	ElseIf currentStage >= 95 && currentStage < 100
		bosz04.SetStage(100)
	ElseIf currentStage >= 75 && currentStage < 80
		bosz04.SetStage(80)
	EndIf
	Debug.Trace("[B21 BoSZ04] VTU terminal stage result=" + bosz04.GetStage() as String, 0)
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
	Debug.Trace("[B21 BoSZ04] VTU terminal Montgomery log selected ref=" + akTerminalRef as String, 0)
	Return
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
	B21RunAutomatedTest(akTerminalRef, "analyze DNA")
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
	B21RunAutomatedTest(akTerminalRef, "divert power")
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
	B21RunAutomatedTest(akTerminalRef, "run automated test")
EndFunction
