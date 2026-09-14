Event OnRead()
	Debug.Trace("[B21 BoSZ04] Mission note read quest=" + pBoSZ04 as String + " keyword=" + pBoSz04_StartKeyword as String, 0)
	If pBoSZ04 == None
		Debug.Trace("[B21 BoSZ04] Mission note has no bound quest", 0)
		Return
	EndIf
	Debug.Trace("[B21 BoSZ04] Mission note quest state running=" + pBoSZ04.IsRunning() as String + " completed=" + pBoSZ04.IsCompleted() as String + " stage=" + pBoSZ04.GetStage() as String, 0)
	If pBoSZ04.IsCompleted()
		Debug.Trace("[B21 BoSZ04] Mission note ignored completed quest", 0)
		Return
	EndIf

	If !pBoSZ04.IsRunning()
		Bool startedFromStory = False
		If pBoSz04_StartKeyword != None
			startedFromStory = pBoSz04_StartKeyword.SendStoryEventAndWait(None, Game.GetPlayer())
			Debug.Trace("[B21 BoSZ04] Mission note story result=" + startedFromStory as String + " running=" + pBoSZ04.IsRunning() as String + " stage=" + pBoSZ04.GetStage() as String, 0)
		EndIf
		If !startedFromStory
			Debug.Trace("[B21 BoSZ04] Mission note falling back to direct quest start", 0)
			pBoSZ04.Start()
			Debug.Trace("[B21 BoSZ04] Mission note direct result running=" + pBoSZ04.IsRunning() as String + " stage=" + pBoSZ04.GetStage() as String, 0)
		EndIf
	EndIf

	If pBoSZ04.IsRunning() && pBoSZ04.GetCurrentStageID() < 75
		Debug.Trace("[B21 BoSZ04] Mission note setting stage 75 from=" + pBoSZ04.GetCurrentStageID() as String, 0)
		pBoSZ04.SetStage(75)
		Debug.Trace("[B21 BoSZ04] Mission note stage result=" + pBoSZ04.GetStage() as String, 0)
	Else
		Debug.Trace("[B21 BoSZ04] Mission note cannot advance running=" + pBoSZ04.IsRunning() as String + " stage=" + pBoSZ04.GetStage() as String, 0)
	EndIf
EndEvent
