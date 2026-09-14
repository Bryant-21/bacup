Function SetBoSZ04Stage(Actor akPlayer)
	Debug.Trace("[B21 BoSZ04] DNA perk entry player=" + akPlayer as String + " quest=" + pBoSZ04 as String, 0)
	If akPlayer == Game.GetPlayer() && pBoSZ04 != None && pBoSZ04.IsRunning()
		Int currentStage = pBoSZ04.GetCurrentStageID()
		Debug.Trace("[B21 BoSZ04] DNA perk quest stage=" + currentStage as String, 0)
		If currentStage >= 100 && currentStage < 200
			pBoSZ04.SetStage(200)
			Debug.Trace("[B21 BoSZ04] DNA perk stage result=" + pBoSZ04.GetStage() as String, 0)
		EndIf
	Else
		Debug.Trace("[B21 BoSZ04] DNA perk ignored invalid player or quest state", 0)
	EndIf
EndFunction
