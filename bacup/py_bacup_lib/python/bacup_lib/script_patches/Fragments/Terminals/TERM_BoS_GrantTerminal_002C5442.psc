Function Fragment_Terminal_01(ObjectReference akTerminalRef)
	Debug.Trace("[B21 BoSZ04] Grant terminal selected ref=" + akTerminalRef as String + " quest=" + pBoSZ04 as String + " keyword=" + pBoSz04_StartKeyword as String, 0)
	If pBoSZ04 == None
		Debug.Trace("[B21 BoSZ04] Grant terminal has no bound quest", 0)
		Return
	EndIf
	Debug.Trace("[B21 BoSZ04] Grant terminal quest state running=" + pBoSZ04.IsRunning() as String + " completed=" + pBoSZ04.IsCompleted() as String + " stage=" + pBoSZ04.GetStage() as String, 0)
	If pBoSZ04.IsCompleted()
		Debug.Trace("[B21 BoSZ04] Grant terminal ignored completed quest", 0)
		Return
	EndIf

	If !pBoSZ04.IsRunning()
		Bool startedFromStory = False
		If pBoSz04_StartKeyword != None
			startedFromStory = pBoSz04_StartKeyword.SendStoryEventAndWait(None, Game.GetPlayer())
			Debug.Trace("[B21 BoSZ04] Grant terminal story result=" + startedFromStory as String + " running=" + pBoSZ04.IsRunning() as String + " stage=" + pBoSZ04.GetStage() as String, 0)
		EndIf
		If !startedFromStory
			Debug.Trace("[B21 BoSZ04] Grant terminal falling back to direct quest start", 0)
			pBoSZ04.Start()
			Debug.Trace("[B21 BoSZ04] Grant terminal direct result running=" + pBoSZ04.IsRunning() as String + " stage=" + pBoSZ04.GetStage() as String, 0)
		EndIf
	EndIf

	If pBoSZ04.IsRunning() && pBoSZ04.GetCurrentStageID() < 50
		Debug.Trace("[B21 BoSZ04] Grant terminal setting stage 50 from=" + pBoSZ04.GetCurrentStageID() as String, 0)
		pBoSZ04.SetStage(50)
		Debug.Trace("[B21 BoSZ04] Grant terminal stage result=" + pBoSZ04.GetStage() as String, 0)
	Else
		Debug.Trace("[B21 BoSZ04] Grant terminal cannot advance running=" + pBoSZ04.IsRunning() as String + " stage=" + pBoSZ04.GetStage() as String, 0)
	EndIf
EndFunction
