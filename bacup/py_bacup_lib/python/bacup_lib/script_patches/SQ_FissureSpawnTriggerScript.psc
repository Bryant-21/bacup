Bool Function PassesSpawnChance()
	Int spawnChance = ChanceToSpawn
	If spawnChance <= 0
		spawnChance = 100
	ElseIf spawnChance > 100
		spawnChance = 100
	EndIf

	Int chanceRoll = debugForceChanceRoll
	If chanceRoll < 0
		chanceRoll = Utility.RandomInt(1, 100)
	EndIf
	Return chanceRoll <= spawnChance
EndFunction

Actor Function SpawnLocalScorchbeast(Actor akPlayer)
	; The converted plugin has no Story Manager nodes, so use its verified LvlScorchBeast form directly.
	ActorBase scorchbeastBase = Game.GetFormFromFile(0x000974BC, "SeventySix.esm") as ActorBase
	If scorchbeastBase == None
		Return None
	EndIf

	Actor spawnedScorchbeast = PlaceAtMe(scorchbeastBase, 1, False, False, True) as Actor
	If spawnedScorchbeast != None && akPlayer != None
		spawnedScorchbeast.StartCombat(akPlayer, True)
	EndIf
	Return spawnedScorchbeast
EndFunction

Function StartTriggerCooldown(Bool abSpawnSucceeded)
	Float cooldownSeconds = TriggerCoolDown
	If abSpawnSucceeded
		cooldownSeconds = TiggerCoolDownOnSuccessfulSpawn
	EndIf
	If cooldownSeconds <= 0.0
		If abSpawnSucceeded
			cooldownSeconds = 300.0
		Else
			cooldownSeconds = 30.0
		EndIf
	EndIf

	CancelTimer(TimerID_CoolDown)
	GoToState("coolingdown")
	StartTimer(cooldownSeconds, TimerID_CoolDown)
EndFunction

Function HandleSpawnTrigger(ObjectReference akActionRef)
	Actor playerRef = akActionRef as Actor
	If playerRef == None || playerRef != Game.GetPlayer()
		Return
	EndIf

	Actor spawnedScorchbeast
	If PassesSpawnChance()
		spawnedScorchbeast = SpawnLocalScorchbeast(playerRef)
	EndIf
	StartTriggerCooldown(spawnedScorchbeast != None)
EndFunction

Event OnTriggerEnter(ObjectReference akActionRef)
	HandleSpawnTrigger(akActionRef)
EndEvent

Event OnActivate(ObjectReference akActionRef)
	HandleSpawnTrigger(akActionRef)
EndEvent

State coolingdown
	Event OnTimer(Int aiTimerID)
		If aiTimerID == TimerID_CoolDown
			GoToState("")
		EndIf
	EndEvent
EndState
