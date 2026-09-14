Function Fragment_Terminal_04(ObjectReference akTerminalRef)
	If FS02_Fruition != None && FS02_Fruition.GetStageDone(700) && !FS02_Fruition.GetStageDone(800)
		FS02_Fruition.SetStage(800)
	EndIf
EndFunction
