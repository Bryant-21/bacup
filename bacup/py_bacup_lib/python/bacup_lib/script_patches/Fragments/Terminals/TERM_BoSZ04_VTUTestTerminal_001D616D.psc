Quest Function GetBoSZ04()
	Return Game.GetFormFromFile(0x00065DFE, "SeventySix.esm") as Quest
EndFunction

Function Fragment_Terminal_01(ObjectReference akTerminalRef)
	Debug.Trace("[B21 BoSZ04] VTU terminal Montgomery log selected ref=" + akTerminalRef as String, 0)
	Return
EndFunction

Function Fragment_Terminal_02(ObjectReference akTerminalRef)
	Quest bosz04 = GetBoSZ04()
	Debug.Trace("[B21 BoSZ04] VTU terminal analyze DNA selected ref=" + akTerminalRef as String + " quest=" + bosz04 as String, 0)
	If bosz04 != None && bosz04.IsRunning()
		Int currentStage = bosz04.GetCurrentStageID()
		Debug.Trace("[B21 BoSZ04] VTU analyze current stage=" + currentStage as String, 0)
		If currentStage >= 300 && currentStage < 350
			bosz04.SetStage(350)
			Debug.Trace("[B21 BoSZ04] VTU analyze stage result=" + bosz04.GetStage() as String, 0)
		EndIf
	Else
		Debug.Trace("[B21 BoSZ04] VTU analyze quest missing or not running", 0)
	EndIf
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
	Quest bosz04 = GetBoSZ04()
	Debug.Trace("[B21 BoSZ04] VTU terminal divert power selected ref=" + akTerminalRef as String + " quest=" + bosz04 as String, 0)
	If bosz04 != None && bosz04.IsRunning()
		Int currentStage = bosz04.GetCurrentStageID()
		Debug.Trace("[B21 BoSZ04] VTU divert power current stage=" + currentStage as String, 0)
		If currentStage >= 75 && currentStage < 80
			bosz04.SetStage(80)
			Debug.Trace("[B21 BoSZ04] VTU divert power stage result=" + bosz04.GetStage() as String, 0)
		EndIf
	Else
		Debug.Trace("[B21 BoSZ04] VTU divert power quest missing or not running", 0)
	EndIf
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
	Quest bosz04 = GetBoSZ04()
	Debug.Trace("[B21 BoSZ04] VTU terminal run automated test selected ref=" + akTerminalRef as String + " quest=" + bosz04 as String, 0)
	If bosz04 != None && bosz04.IsRunning()
		Int currentStage = bosz04.GetCurrentStageID()
		Debug.Trace("[B21 BoSZ04] VTU test current stage=" + currentStage as String, 0)
		If currentStage >= 95 && currentStage < 100
			bosz04.SetStage(100)
			Debug.Trace("[B21 BoSZ04] VTU test stage result=" + bosz04.GetStage() as String, 0)
		EndIf
	Else
		Debug.Trace("[B21 BoSZ04] VTU test quest missing or not running", 0)
	EndIf
EndFunction
