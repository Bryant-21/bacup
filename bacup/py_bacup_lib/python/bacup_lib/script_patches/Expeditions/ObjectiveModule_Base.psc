Int Function GetWaitingStage()
	Return XPD_ObjectiveModule_WaitingStage_Global.GetValue() as Int
EndFunction

Int Function GetActiveStage()
	Return XPD_ObjectiveModule_ActiveStage_Global.GetValue() as Int
EndFunction

Int Function GetCompleteStage()
	Return XPD_ObjectiveModule_CompleteStage_Global.GetValue() as Int
EndFunction
