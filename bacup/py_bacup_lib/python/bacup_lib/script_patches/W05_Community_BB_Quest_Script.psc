Event OnQuestInit()
	RefreshCannibalReferences()
	RegisterForRemoteEvent(Cannibal01Alias, "OnDeath")
	RegisterForRemoteEvent(Cannibal02Alias, "OnDeath")
	RegisterForRemoteEvent(Cannibal03Alias, "OnDeath")
	RegisterForRemoteEvent(RoomDoor, "OnActivate")
	RegisterForPlayerSleep()
EndEvent

Event OnQuestShutdown()
	UnregisterForPlayerSleep()
	UnregisterForAllRemoteEvents()
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
	HandleQuestStage(auiStageID)
EndEvent

Event OnPlayerSleepStart(Float afSleepStartTime, Float afDesiredSleepEndTime, ObjectReference akBed)
	If RoomRented && akBed == RoomBed.GetReference() && !IsStageDone(20)
		SetStage(20)
	EndIf
EndEvent

Event OnPlayerSleepStop(Bool abInterrupted, ObjectReference akBed)
	If abInterrupted && akBed == RoomBed.GetReference() && IsStageDone(20) && !IsStageDone(60) && !IsStageDone(70)
		InteruptSit = True
		If NPCCompletePath
			StartInterruptedScene()
		EndIf
	EndIf
EndEvent

Event ReferenceAlias.OnActivate(ReferenceAlias akSender, ObjectReference akActionRef)
	If akSender == RoomDoor && akActionRef == currentPlayer.GetReference() && IsStageDone(20) && !IsStageDone(60) && !IsStageDone(70)
		InteruptSit = True
		If W05_Community_BB_Quest_Door_Scene.IsPlaying()
			W05_Community_BB_Quest_Door_Scene.Stop()
		EndIf
		W05_Community_BB_Quest_Interrupt_OpenDoor.ForceStart()
	EndIf
EndEvent

Event ReferenceAlias.OnDeath(ReferenceAlias akSender, Actor akKiller)
	If akSender == Cannibal01Alias || akSender == Cannibal02Alias || akSender == Cannibal03Alias
		If IsStageDone(70) && !IsStageDone(100)
			ActorsDead += 1
			If ActorsDead >= 3
				SetStage(100)
			EndIf
		ElseIf !IsStageDone(80)
			ActorsDead = CountDeadCannibals()
			If ActorsDead >= 3
				SetStage(80)
			EndIf
		EndIf
	EndIf
EndEvent

Function HandleQuestStage(Int aiStage)
	If aiStage == 10 || aiStage == 15
		GrantRoomAccess()
	ElseIf aiStage == 20
		NPCCompletePath = False
		InteruptSit = False
		ActorsDead = 0
		RefreshCannibalReferences()
		EvaluateCannibalPackages()
	ElseIf aiStage == 60 || aiStage == 65
		RoomRented = False
		ActorsDead = CountDeadCannibals()
		If aiStage == 65
			ObjectReference basementMarker = BasementEnableMarker.GetReference()
			ObjectReference basementDoorRef = BasementDoor.GetReference()
			If basementMarker
				basementMarker.Enable()
			EndIf
			If basementDoorRef
				basementDoorRef.Lock(False)
			EndIf
		EndIf
		MakeCannibalsHostile()
		If ActorsDead >= 3 && !IsStageDone(80)
			SetStage(80)
		EndIf
	ElseIf aiStage == 70
		RoomRented = False
		NPCCompletePath = False
		ActorsDead = 0
		MakeCannibalsFlee()
	ElseIf aiStage == 80
		If !IsStageDone(100)
			SetStage(100)
		EndIf
	ElseIf aiStage == 100
		CleanupEncounter()
	EndIf
EndFunction

Function CannibalReachedRoom(Actor akActor)
	If akActor && IsStageDone(20) && !IsStageDone(60) && !IsStageDone(70) && !NPCCompletePath
		ActorsDead += 1
		If ActorsDead >= 3
			ActorsDead = 0
			NPCCompletePath = True
			If InteruptSit
				StartInterruptedScene()
			ElseIf !W05_Community_BB_Quest_Door_Scene.IsPlaying()
				W05_Community_BB_Quest_Door_Scene.Start()
			EndIf
		EndIf
	EndIf
EndFunction

Function CannibalFinishedFleeing(Actor akActor)
	If akActor && IsStageDone(70) && !IsStageDone(100)
		akActor.Disable()
		ActorsDead += 1
		If ActorsDead >= 3
			NPCCompletePath = True
			SetStage(100)
		EndIf
	EndIf
EndFunction

Function RefreshCannibalReferences()
	Cannibal01Ref = Cannibal01Alias.GetActorReference()
	Cannibal02Ref = Cannibal02Alias.GetActorReference()
	Cannibal03Ref = Cannibal03Alias.GetActorReference()
EndFunction

Function GrantRoomAccess()
	RoomRented = True
	RegisterForPlayerSleep()
	Actor playerRef = currentPlayer.GetActorReference()
	ObjectReference roomBedRef = RoomBed.GetReference()
	ObjectReference roomDoorRef = RoomDoor.GetReference()
	ObjectReference porchDoorRef = PorchDoor.GetReference()
	If playerRef && LC102_HouseKey && playerRef.GetItemCount(LC102_HouseKey) == 0
		playerRef.AddItem(LC102_HouseKey, 1, True)
	EndIf
	If playerRef && roomBedRef
		roomBedRef.SetActorOwner(playerRef.GetActorBase())
	EndIf
	If roomDoorRef
		roomDoorRef.Lock(False)
	EndIf
	If porchDoorRef
		porchDoorRef.Lock(False)
	EndIf
EndFunction

Function EvaluateCannibalPackages()
	If Cannibal01Ref && !Cannibal01Ref.IsDead()
		Cannibal01Ref.EvaluatePackage()
	EndIf
	If Cannibal02Ref && !Cannibal02Ref.IsDead()
		Cannibal02Ref.EvaluatePackage()
	EndIf
	If Cannibal03Ref && !Cannibal03Ref.IsDead()
		Cannibal03Ref.EvaluatePackage()
	EndIf
EndFunction

Function StartInterruptedScene()
	If W05_Community_BB_Quest_Door_Scene.IsPlaying()
		W05_Community_BB_Quest_Door_Scene.Stop()
	EndIf
	If !W05_Community_BB_Quest_Interrupt.IsPlaying()
		W05_Community_BB_Quest_Interrupt.ForceStart()
	EndIf
EndFunction

Function MakeCannibalsHostile()
	Actor playerRef = currentPlayer.GetActorReference()
	MakeCannibalHostile(Cannibal01Ref, playerRef)
	MakeCannibalHostile(Cannibal02Ref, playerRef)
	MakeCannibalHostile(Cannibal03Ref, playerRef)
EndFunction

Function MakeCannibalHostile(Actor cannibalRef, Actor playerRef)
	If cannibalRef && !cannibalRef.IsDead()
		cannibalRef.RemoveFromFaction(W05_Community_BB_Cannibal_Faction)
		cannibalRef.AddToFaction(W05_Community_BB_CannibalEnemy_Faction)
		If playerRef
			cannibalRef.StartCombat(playerRef)
		EndIf
	EndIf
EndFunction

Function MakeCannibalsFlee()
	MakeCannibalFlee(Cannibal01Ref)
	MakeCannibalFlee(Cannibal02Ref)
	MakeCannibalFlee(Cannibal03Ref)
EndFunction

Function MakeCannibalFlee(Actor cannibalRef)
	If cannibalRef && !cannibalRef.IsDead()
		cannibalRef.StopCombatAlarm()
		cannibalRef.RemoveFromFaction(W05_Community_BB_CannibalEnemy_Faction)
		cannibalRef.AddToFaction(W05_Community_BB_Cannibal_Faction)
		cannibalRef.EvaluatePackage()
	ElseIf cannibalRef
		ActorsDead += 1
	EndIf
EndFunction

Int Function CountDeadCannibals()
	Int deadCount = 0
	If !Cannibal01Ref || Cannibal01Ref.IsDead()
		deadCount += 1
	EndIf
	If !Cannibal02Ref || Cannibal02Ref.IsDead()
		deadCount += 1
	EndIf
	If !Cannibal03Ref || Cannibal03Ref.IsDead()
		deadCount += 1
	EndIf
	Return deadCount
EndFunction

Function CleanupEncounter()
	RoomRented = False
	UnregisterForPlayerSleep()
	UnregisterForAllRemoteEvents()
	Actor playerRef = currentPlayer.GetActorReference()
	If playerRef && LC102_HouseKey && playerRef.GetItemCount(LC102_HouseKey) > 0
		playerRef.RemoveItem(LC102_HouseKey, 1, True)
	EndIf
	If curRenterAlias
		curRenterAlias.Clear()
	EndIf
	StopAllScenes()
EndFunction

Function StopAllScenes()
	If W05_Community_BB_Quest_Door_Scene.IsPlaying()
		W05_Community_BB_Quest_Door_Scene.Stop()
	EndIf
	If W05_Community_BB_Quest_Interrupt.IsPlaying()
		W05_Community_BB_Quest_Interrupt.Stop()
	EndIf
	If W05_Community_BB_Quest_Interrupt_OpenDoor.IsPlaying()
		W05_Community_BB_Quest_Interrupt_OpenDoor.Stop()
	EndIf
	If W05_Community_BB_Quest_BargeIn.IsPlaying()
		W05_Community_BB_Quest_BargeIn.Stop()
	EndIf
EndFunction
