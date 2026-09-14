Function BeginLocalGruntHunt()
	playerRef = Alias_Player.GetActorReference()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If UpdateNumOfActiveDailyBounties != None
		currentActiveDailyBounties = UpdateNumOfActiveDailyBounties.GetValue() + 1.0
		UpdateNumOfActiveDailyBounties.SetValue(currentActiveDailyBounties)
	EndIf
EndFunction

Function PrepareLocalGruntTargets()
	Burn:Burn_Bounty:Burn_Bounty_GruntSpawnScript spawnScript = (Self as Quest) as Burn:Burn_Bounty:Burn_Bounty_GruntSpawnScript
	If spawnScript != None
		spawnScript.PrepareLocalTargets()
	ElseIf !IsStageDone(9990)
		SetStage(9990)
	EndIf
EndFunction

Function EngageLocalGruntTargets()
	Burn:Burn_Bounty:Burn_Bounty_GruntSpawnScript spawnScript = (Self as Quest) as Burn:Burn_Bounty:Burn_Bounty_GruntSpawnScript
	If spawnScript != None
		spawnScript.EngageLocalTargets()
	EndIf
EndFunction

Function RecordLocalGruntCompletion()
	If playerRef == None
		playerRef = Game.GetPlayer()
	EndIf
	If playerRef != None && NumOfGruntHuntsCompletedAV != None
		playerRef.ModValue(NumOfGruntHuntsCompletedAV, 1.0)
	EndIf
EndFunction

Function CleanupLocalGruntHunt()
	If UpdateNumOfActiveDailyBounties != None
		Float activeCount = UpdateNumOfActiveDailyBounties.GetValue()
		If activeCount > 0.0
			UpdateNumOfActiveDailyBounties.SetValue(activeCount - 1.0)
		EndIf
	EndIf
EndFunction
