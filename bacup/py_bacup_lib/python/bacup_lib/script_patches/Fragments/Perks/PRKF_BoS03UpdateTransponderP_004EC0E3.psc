Function SetBoS03Stage(Actor akPlayer, Int nStage)
	If pQSTBoS03TransponderOn != None && akPlayer != None
		pQSTBoS03TransponderOn.Play(akPlayer as ObjectReference)
	EndIf
	If pBoS03 != None && nStage > 0 && !pBoS03.IsStageDone(nStage)
		pBoS03.SetStage(nStage)
	EndIf
EndFunction
