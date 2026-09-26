Function CompleteIfAllTargetsPlaced()
	If IsStageDone(11) && IsStageDone(12) && IsStageDone(13) && IsStageDone(14) && IsStageDone(15) && !IsStageDone(1000)
		SetStage(1000)
	EndIf
EndFunction
