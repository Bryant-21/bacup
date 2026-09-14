Actor Function ActorFromAlias(ReferenceAlias actorAlias)
	If actorAlias == None
		Return None
	EndIf
	Return actorAlias.GetActorReference()
EndFunction

Function SelectSceneLocation()
	If ChosenLocation != None && SceneCenter.GetReference() != None
		Return
	EndIf
	If Locations == None || Locations.Length == 0
		Return
	EndIf

	Int startIndex = Utility.RandomInt(0, Locations.Length - 1)
	Int offset = 0
	While offset < Locations.Length
		Int index = (startIndex + offset) % Locations.Length
		SceneLocation candidate = Locations[index]
		If candidate != None && candidate.AreaLocations != None && candidate.AreaLocations.GetCount() > 0
			ObjectReference marker = candidate.AreaLocations.GetAt(Utility.RandomInt(0, candidate.AreaLocations.GetCount() - 1))
			If marker != None
				ChosenLocation = candidate
				SceneCenter.ForceRefTo(marker)
				If candidate.StageToSet > 0 && !IsStageDone(candidate.StageToSet)
					SetStage(candidate.StageToSet)
				EndIf
				Return
			EndIf
		EndIf
		offset += 1
	EndWhile
EndFunction

Function MoveActorToMarker(ReferenceAlias actorAlias, ReferenceAlias markerAlias)
	Actor actorRef = ActorFromAlias(actorAlias)
	ObjectReference markerRef
	If markerAlias != None
		markerRef = markerAlias.GetReference()
	EndIf
	If actorRef != None
		actorRef.Enable()
		If markerRef != None
			actorRef.MoveTo(markerRef)
		EndIf
		actorRef.EvaluatePackage()
	EndIf
EndFunction

Function MoveActorToSceneCenter(ReferenceAlias actorAlias)
	MoveActorToMarker(actorAlias, SceneCenter)
EndFunction

Function EquipActor(ReferenceAlias actorAlias, Armor outfit, Armor headwear = None)
	Actor actorRef = ActorFromAlias(actorAlias)
	If actorRef == None
		Return
	EndIf
	If outfit != None
		actorRef.AddItem(outfit, 1, True)
		actorRef.EquipItem(outfit, False, True)
	EndIf
	If headwear != None
		actorRef.AddItem(headwear, 1, True)
		actorRef.EquipItem(headwear, False, True)
	EndIf
EndFunction

Function StartLocalScene(Scene sceneToStart)
	If sceneToStart != None && !sceneToStart.IsPlaying()
		sceneToStart.Start()
	EndIf
EndFunction

Function StopLocalScene(Scene sceneToStop)
	If sceneToStop != None && sceneToStop.IsPlaying()
		sceneToStop.Stop()
	EndIf
EndFunction

Function RecordEncounter(ReferenceAlias playerAlias, ActorValue encounterCount)
	Actor player = ActorFromAlias(playerAlias)
	If player != None && encounterCount != None
		player.SetValue(encounterCount, player.GetValue(encounterCount) + 1.0)
	EndIf
EndFunction

Function ClearLocalSelection()
	If SceneCenter != None
		SceneCenter.Clear()
	EndIf
	ChosenLocation = None
EndFunction

Int Function LocalStopTimerID()
	Return 2140
EndFunction

Function ScheduleLocalStop()
	Actor player = Game.GetPlayer()
	If player != None
		RegisterForRemoteEvent(player, "OnPlayerLoadGame")
	EndIf
	CancelTimer(LocalStopTimerID())
	StartTimer(2.0, LocalStopTimerID())
EndFunction

Event OnTimer(Int aiTimerID)
	If aiTimerID == LocalStopTimerID() && IsRunning() && IsCompleted()
		Stop()
	EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
	If akSender == Game.GetPlayer() && IsRunning() && IsCompleted()
		ScheduleLocalStop()
	EndIf
EndEvent

Event OnQuestShutdown()
	CancelTimer(LocalStopTimerID())
	Actor player = Game.GetPlayer()
	If player != None
		UnregisterForRemoteEvent(player, "OnPlayerLoadGame")
	EndIf
	ClearLocalSelection()
EndEvent
