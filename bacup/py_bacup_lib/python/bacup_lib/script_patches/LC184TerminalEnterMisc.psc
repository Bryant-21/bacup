Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
	If LC184_WestTekMisc != None && StageToSet >= 0
		LC184_WestTekMisc.SetStage(StageToSet)
	EndIf
EndEvent
