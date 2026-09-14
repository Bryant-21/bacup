Function StartMissionQuestExitTransition2()
	If currentExitData != None
		currentExitData.ExitStartIMod.PopTo(currentExitData.ExitHoldIMod, 1.0)
		; FO76 SendRMIToServer("PerformMissionQuestExit") -> direct local call.
		Self.PerformMissionQuestExit(Game.GetPlayer())
	EndIf
EndFunction

Function FinishMissionQuestExitTransition2()
	If currentExitData != None
		Self.CleanupTransition()
		; FO76 SendRMIToServer("ClearMissionQuestExit") -> direct local call.
		Self.ClearMissionQuestExit(Game.GetPlayer())
	EndIf
EndFunction
