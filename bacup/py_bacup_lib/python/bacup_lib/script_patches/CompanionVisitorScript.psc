Event OnLoad()
	If IsDead()
		AllyCleanedUp = True
		CancelTimer(TimerID_CampVisitorCheck)
		CleanupExpiredVisitors(True)
		Return
	EndIf
	AllyCleanedUp = False
	If SpawnedActors == None
		SpawnedActors = new Actor[0]
	EndIf
	CancelTimer(TimerID_CampVisitorCheck)
	StartTimer(TimerDur_CampVisitorCheck, TimerID_CampVisitorCheck)
EndEvent

Event OnUnload()
	CancelTimer(TimerID_CampVisitorCheck)
	CleanupExpiredVisitors(True)
EndEvent

Event OnDeath(Actor akKiller)
	AllyCleanedUp = True
	CancelTimer(TimerID_CampVisitorCheck)
	CleanupExpiredVisitors(True)
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID != TimerID_CampVisitorCheck || AllyCleanedUp
		Return
	EndIf
	CheckAndProcessVisitors()
	StartTimer(TimerDur_CampVisitorCheck, TimerID_CampVisitorCheck)
EndEvent

Function CheckAndProcessVisitors()
	If CheckAndProcessVisitorsLock
		Return
	EndIf
	CheckAndProcessVisitorsLock = True
	CleanupExpiredVisitors(False)

	Actor player = Game.GetPlayer()
	If player && SpawnedVisitorData != None
		Int index = 0
		While index < SpawnedVisitorData.Length
			SpawnVisitorDatum currentDatum = SpawnedVisitorData[index]
			If VisitorDatumMatches(player, currentDatum) && (currentDatum.allowMultipleRefs || !HasSpawnedVisitor(currentDatum.VisitorToSpawn))
				If Utility.RandomInt(1, 100) <= currentDatum.SpawnChance
					SpawnVisitor(currentDatum)
				EndIf
			EndIf
			index += 1
		EndWhile
	EndIf
	CheckAndProcessVisitorsLock = False
EndFunction

Bool Function VisitorDatumMatches(Actor player, SpawnVisitorDatum currentDatum)
	If !currentDatum.VisitorToSpawn
		Return False
	EndIf
	If !currentDatum.PlayerAV
		Return True
	EndIf
	Float playerValue = player.GetValue(currentDatum.PlayerAV)
	If currentDatum.PlayerAV_GreaterThanOrEqualTo
		Return playerValue >= currentDatum.PlayerAV_Value
	EndIf
	Return playerValue == currentDatum.PlayerAV_Value
EndFunction

Bool Function HasSpawnedVisitor(ActorBase visitorBase)
	If !visitorBase || SpawnedActors == None
		Return False
	EndIf
	Int index = 0
	While index < SpawnedActors.Length
		If SpawnedActors[index] && SpawnedActors[index].GetActorBase() == visitorBase
			Return True
		EndIf
		index += 1
	EndWhile
	Return False
EndFunction

Function SpawnVisitor(SpawnVisitorDatum currentDatum)
	Actor visitor = PlaceAtMe(currentDatum.VisitorToSpawn, 1, False, False) as Actor
	If !visitor
		Return
	EndIf

	If currentDatum.ExpiryDayAV
		visitor.SetValue(currentDatum.ExpiryDayAV, Utility.GetCurrentGameTime() + currentDatum.ExpiryDay)
	EndIf
	ObjectReference workshop = GetLinkedRef()
	If workshop && CampVistorToWorkshopLink
		visitor.SetLinkedRef(workshop, CampVistorToWorkshopLink)
	EndIf
	SpawnedActors.Add(visitor)
	Visitors = SpawnedActors

	If COMP_VisitorStart
		COMP_VisitorStart.SendStoryEventAndWait(GetCurrentLocation(), Self, visitor)
	EndIf
EndFunction

Function CleanupExpiredVisitors(Bool deletingForUnload)
	If SpawnedActors == None
		Return
	EndIf
	Float currentDay = Utility.GetCurrentGameTime()
	Int index = 0
	While index < SpawnedActors.Length
		Actor visitor = SpawnedActors[index]
		If visitor
			SpawnVisitorDatum visitorDatum = FindVisitorDatum(visitor.GetActorBase())
			Bool expired = visitorDatum.ExpiryDayAV && visitor.GetValue(visitorDatum.ExpiryDayAV) <= currentDay
			Bool deleteNow = expired || (deletingForUnload && visitorDatum.DeleteWhenUnloaded)
			If deleteNow
				visitor.Disable()
				visitor.Delete()
				SpawnedActors[index] = None
			EndIf
		EndIf
		index += 1
	EndWhile
	Visitors = SpawnedActors
EndFunction

SpawnVisitorDatum Function FindVisitorDatum(ActorBase visitorBase)
	SpawnVisitorDatum emptyDatum = new SpawnVisitorDatum
	If SpawnedVisitorData == None
		Return emptyDatum
	EndIf
	Int index = 0
	While index < SpawnedVisitorData.Length
		If SpawnedVisitorData[index].VisitorToSpawn == visitorBase
			Return SpawnedVisitorData[index]
		EndIf
		index += 1
	EndWhile
	Return emptyDatum
EndFunction
