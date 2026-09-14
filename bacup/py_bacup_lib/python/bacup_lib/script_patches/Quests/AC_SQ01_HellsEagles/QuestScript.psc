Function ResetLocalQuestState()
	Actor player = Alias_Player.GetActorReference()
	If player != None && AC_SQ01_LittleRob_Anger_AV != None
		player.SetValue(AC_SQ01_LittleRob_Anger_AV, 0.0)
	EndIf

	Actor surly = Alias_Actor_Surly.GetActorReference()
	If surly != None && BloodEagleFaction != None
		surly.RemoveFromFaction(BloodEagleFaction)
		surly.StopCombat()
		surly.EvaluatePackage()
	EndIf
EndFunction

Event OnQuestInit()
	ResetLocalQuestState()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	If auiStageID == CleanupStage
		ResetLocalQuestState()
	EndIf
EndEvent

Event OnQuestShutdown()
	ResetLocalQuestState()
EndEvent
