Function CheckForAllBarrels()
	If IsStageDone(Barrel01Stage) && IsStageDone(Barrel02Stage) && IsStageDone(Barrel03Stage) && !IsStageDone(AllBarrelsStage)
		SetStage(AllBarrelsStage)
	EndIf
EndFunction

Function CheckForAllDumped()
	If IsStageDone(Dumped01Stage) && IsStageDone(Dumped02Stage) && IsStageDone(Dumped03stage) && !IsStageDone(AllDumpedStage)
		SetStage(AllDumpedStage)
	EndIf
EndFunction
