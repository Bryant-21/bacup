Function AddClue(Int aiClue)
	If FoundClues == None
		FoundClues = new Int[0]
	EndIf
	If FoundClues.Find(aiClue) >= 0
		Return
	EndIf

	FoundClues.Add(aiClue)
	SetObjectiveDisplayed(300, True, True)
	If FoundClues.Length >= CluesNeeded && !GetStageDone(StageToSetWhenAllCluesFound)
		SetStage(StageToSetWhenAllCluesFound)
	EndIf
EndFunction

Function GiveVirusToPlayer()
	Actor playerRef = Game.GetPlayer()
	If playerRef != None && CB04_Virus != None && playerRef.GetItemCount(CB04_Virus) == 0
		playerRef.AddItem(CB04_Virus, 1, True)
	EndIf
EndFunction

Function SpawnRobotWave(Int aiWave)
	ObjectReference spawnMarker = Game.GetFormFromFile(0x004ECF00, "SeventySix.esm") as ObjectReference
	ActorBase robotBase = Game.GetFormFromFile(0x00146CDF, "Fallout4.esm") as ActorBase
	If aiWave >= 3
		robotBase = Game.GetFormFromFile(0x00121D52, "Fallout4.esm") as ActorBase
	EndIf
	If spawnMarker == None || robotBase == None
		Return
	EndIf

	Int robotCount = 2
	If aiWave >= 4
		robotCount = 3
	EndIf

	Int i = 0
	While i < robotCount
		Actor robotRef = spawnMarker.PlaceAtMe(robotBase, 1, True, False, True) as Actor
		If robotRef != None
			If EncWaveRobots != None
				EncWaveRobots.AddRef(robotRef)
			EndIf
			robotRef.StartCombat(Game.GetPlayer())
		EndIf
		i += 1
	EndWhile
EndFunction

Function CleanUpRobotWaves()
	If EncWaveRobots == None
		Return
	EndIf

	Int i = EncWaveRobots.GetCount() - 1
	While i >= 0
		Actor robotRef = EncWaveRobots.GetAt(i) as Actor
		If robotRef != None
			robotRef.StopCombat()
			robotRef.Disable(False)
			robotRef.Delete()
		EndIf
		i -= 1
	EndWhile
	EncWaveRobots.RemoveAll()
EndFunction

Function StartUploadDefense()
	CancelTimer(101)
	CancelTimer(102)
	CancelTimer(103)
	CancelTimer(104)
	CancelTimer(199)
	CleanUpRobotWaves()

	SpawnRobotWave(1)
	StartTimer(60.0, 101)
	StartTimer(120.0, 102)
	StartTimer(180.0, 103)
	StartTimer(240.0, 104)
	StartTimer(300.0, 199)
EndFunction

Function FinishUploadDefense()
	CancelTimer(101)
	CancelTimer(102)
	CancelTimer(103)
	CancelTimer(104)
	CancelTimer(199)
	CleanUpRobotWaves()
EndFunction

Function GrantCompletionReward()
	Actor playerRef = Game.GetPlayer()
	If playerRef == None
		Return
	EndIf

	If CB04_Reward_CombinationSafeKey != None && playerRef.GetItemCount(CB04_Reward_CombinationSafeKey) == 0
		playerRef.AddItem(CB04_Reward_CombinationSafeKey, 1, False)
	EndIf
	If CB04_Virus != None && playerRef.GetItemCount(CB04_Virus) > 0
		playerRef.RemoveItem(CB04_Virus, playerRef.GetItemCount(CB04_Virus), True)
	EndIf
EndFunction

Event OnTimer(Int aiTimerID)
	If GetCurrentStageID() != StageToSetWhenUploadingVirus
		Return
	EndIf

	If aiTimerID == 101
		SpawnRobotWave(2)
	ElseIf aiTimerID == 102
		SpawnRobotWave(3)
	ElseIf aiTimerID == 103
		SpawnRobotWave(4)
	ElseIf aiTimerID == 104
		SpawnRobotWave(5)
	ElseIf aiTimerID == 199
		SetStage(StageQuestCompleteSetActorValue)
	EndIf
EndEvent
