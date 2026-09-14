Event OnQuestInit()
	EvaluateConfiguredDialogueModules(True, GetStage())
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	EvaluateConfiguredDialogueModules(False, auiStageID)
EndEvent

Function EvaluateConfiguredDialogueModules(Bool abQuestInitializing, Int aiStage)
	Actor playerRef = Game.GetPlayer()
	Location playerLocation = None
	If playerRef != None
		playerLocation = playerRef.GetCurrentLocation()
	EndIf

	Int index = 0
	While ModulesToStart != None && index < ModulesToStart.Length
		ModuleDatum moduleData = ModulesToStart[index]
		Bool timingMatches = (abQuestInitializing && moduleData.StartOnQuestInit) || (!abQuestInitializing && moduleData.StageToStart == aiStage)
		Bool locationMatches = moduleData.ModuleLoc == None || moduleData.ModuleLoc == playerLocation
		If timingMatches && locationMatches
			StartLocalDialogueModule(moduleData)
		EndIf
		index += 1
	EndWhile
EndFunction

Bool Function StartLocalDialogueModule(ModuleDatum akModuleData)
	; QMDL execution is a FO76 service. Mission fragments own any local dialogue quest substitute.
	Return False
EndFunction
