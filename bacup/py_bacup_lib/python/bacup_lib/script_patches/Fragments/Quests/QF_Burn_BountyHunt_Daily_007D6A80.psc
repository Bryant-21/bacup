Burn:Burn_Bounty:Burn_BountyHunt_QuestScript Function LocalQuestScript()
	Return (Self as Quest) as Burn:Burn_Bounty:Burn_BountyHunt_QuestScript
EndFunction

Function Fragment_Stage_0000_Item_00()
	Burn:Burn_Bounty:Burn_BountyHunt_QuestScript questScript = LocalQuestScript()
	If questScript != None
		questScript.BeginLocalGruntHunt()
	EndIf
	If !IsStageDone(50)
		SetStage(50)
	EndIf
EndFunction

Function Fragment_Stage_0050_Item_00()
	If !IsStageDone(100)
		SetStage(100)
	EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
	SetObjectiveDisplayed(10, True)
	ObjectReference playerRef = Alias_Player.GetReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If UIBountyHuntAccept != None && playerRef != None
		UIBountyHuntAccept.Play(playerRef)
	EndIf
EndFunction

Function Fragment_Stage_0200_Item_00()
	Burn:Burn_Bounty:Burn_BountyHunt_QuestScript questScript = LocalQuestScript()
	If questScript != None
		questScript.PrepareLocalGruntTargets()
	ElseIf !IsStageDone(9990)
		SetStage(9990)
	EndIf
EndFunction

Function Fragment_Stage_0250_Item_00()
	SetObjectiveCompleted(10, True)
	SetObjectiveDisplayed(20, True)
	Burn:Burn_Bounty:Burn_BountyHunt_QuestScript questScript = LocalQuestScript()
	If questScript != None
		questScript.EngageLocalGruntTargets()
	EndIf
EndFunction

Function Fragment_Stage_0300_Item_00()
	SetObjectiveCompleted(20, True)
	If !IsStageDone(9000)
		SetStage(9000)
	EndIf
EndFunction

Function Fragment_Stage_9000_Item_00()
	CompleteAllObjectives()
	Burn:Burn_Bounty:Burn_BountyHunt_QuestScript questScript = LocalQuestScript()
	If questScript != None
		questScript.RecordLocalGruntCompletion()
	EndIf
	If !IsStageDone(9999)
		SetStage(9999)
	EndIf
EndFunction

Function Fragment_Stage_9990_Item_00()
	SetObjectiveFailed(10, True)
	SetObjectiveFailed(20, True)
	If !IsStageDone(9999)
		SetStage(9999)
	EndIf
EndFunction

Function Fragment_Stage_9999_Item_00()
	Burn:Burn_Bounty:Burn_BountyHunt_QuestScript questScript = LocalQuestScript()
	If questScript != None
		questScript.CleanupLocalGruntHunt()
	EndIf
	Stop()
EndFunction
