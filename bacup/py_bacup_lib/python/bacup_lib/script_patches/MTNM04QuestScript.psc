Function StartSceneOnce(Scene akScene)
	If akScene != None && !akScene.IsPlaying()
		akScene.Start()
	EndIf
EndFunction

Function SetQuestVariable(String asName, Int aiValue)
	If asName == ""
		Return
	EndIf
	Quest owner = Self as Quest
	B21:QuestVariables questVariables = owner as B21:QuestVariables
	If questVariables != None
		questVariables.SetVariable(asName, aiValue as Float)
	EndIf
EndFunction

DefaultQuestEncounterWaveScript Function WaveScript()
	Quest owner = Self as Quest
	Return owner as DefaultQuestEncounterWaveScript
EndFunction

Function StartWave(Int aiWaveIndex)
	DefaultQuestEncounterWaveScript waves = WaveScript()
	If waves != None && aiWaveIndex >= 0
		waves.StartEncounterWave(aiWaveIndex)
	EndIf
EndFunction

Function StopWave(Int aiWaveIndex)
	DefaultQuestEncounterWaveScript waves = WaveScript()
	If waves != None && aiWaveIndex >= 0
		waves.StopEncounterWave(aiWaveIndex, False)
	EndIf
EndFunction

Bool Function IsActivityRunning()
	Return IsRunning() && IsStageDone(230) && !IsStageDone(ActivityDoneStage) && !IsStageDone(FailureStage)
EndFunction

Function SetCollectionEnabled(RefCollectionAlias akCollection, Bool abEnabled)
	If akCollection == None
		Return
	EndIf
	Int index = 0
	Int count = akCollection.GetCount()
	While index < count
		ObjectReference member = akCollection.GetAt(index)
		If member != None
			If abEnabled
				member.Enable(False)
			Else
				member.Disable(False)
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

Function DeleteCollectionRefs(RefCollectionAlias akCollection)
	If akCollection == None
		Return
	EndIf
	Int index = akCollection.GetCount() - 1
	While index >= 0
		ObjectReference member = akCollection.GetAt(index)
		If member != None
			UnregisterForRemoteEvent(member, "OnActivate")
			member.Disable(False)
			member.Delete()
		EndIf
		index -= 1
	EndWhile
	akCollection.RemoveAll()
EndFunction

; ---- Setup -------------------------------------------------------------------------------------

Function ResetActivity()
	Int timerID = 1
	While timerID <= 13
		CancelTimer(timerID)
		timerID += 1
	EndWhile
	UndesirablesInRoom = False
	UndesirableBreachedSpinLock = False
	IncrementCenterpiecesSpinLock = False
	IncrementPlacesettingsSpinLock = False
	SendRobotToWorkSpinLock = False
	iUnDesirablePenaltyCount = 0
	iUndesirablesBreached = 0
	iRewardPoints = 5
	iRobotsWorkingCurrent = 0
	iRobotsTotal = 5
	If RobotData != None
		iRobotsTotal = RobotData.Length
	EndIf
	iRobotsCurrentTotal = iRobotsTotal
	B21ScenePollTicks = 0
	iPlacesettingsCurrent = 0
	iCenterpiecesCurrent = 0
	bActivityStarted = False
	bRobotArrived = False
	bRobotDestroyed = False

	DeleteCollectionRefs(MisplacedPlacesettingActivators)
	DeleteCollectionRefs(MisplacedCenterpieceActivators)
	SetCollectionEnabled(StaticPlacesettings, False)
	SetCollectionEnabled(StaticCenterpieces, False)
	If FinishedTables != None
		FinishedTables.RemoveAll()
	EndIf
	iPlacesettingsStaticTotal = 24
	If StaticPlacesettings != None && StaticPlacesettings.GetCount() > 0
		iPlacesettingsStaticTotal = StaticPlacesettings.GetCount()
	EndIf
	iCenterpiecesStaticTotal = 6
	If StaticCenterpieces != None && StaticCenterpieces.GetCount() > 0
		iCenterpiecesStaticTotal = StaticCenterpieces.GetCount()
	EndIf
	PublishCounts()

	Actor billingsleyRef = None
	If Billingsley != None
		billingsleyRef = Billingsley.GetActorReference()
	EndIf
	If billingsleyRef != None
		; FO76 opens Billingsley's intro from his greeting; activating him is the dependable FO4 trigger.
		RegisterForRemoteEvent(billingsleyRef, "OnActivate")
	EndIf
EndFunction

Function PublishCounts()
	SetQuestVariable(WorkingRobots, iRobotsWorkingCurrent)
	SetQuestVariable(AllRobots, iRobotsCurrentTotal)
	SetQuestVariable(PlacesettingsPlaced, iPlacesettingsCurrent)
	SetQuestVariable(PlacesettingsToPlace, iPlacesettingsStaticTotal)
	SetQuestVariable(CenterpiecesPlaced, iCenterpiecesCurrent)
	SetQuestVariable(CenterpiecesToPlace, iCenterpiecesStaticTotal)
EndFunction

; ---- Billingsley -------------------------------------------------------------------------------

Function PlayIntroScene()
	StartSceneOnce(MTNM04_Billingsley_IntroScene)
	B21ScenePollTicks = 0
	StartTimer(1.0, 10)
EndFunction

Function FinishIntro()
	CancelTimer(10)
	If IsRunning() && !IsStageDone(230) && !IsStageDone(9990)
		SetStage(230)
	EndIf
EndFunction

; ---- Robot waitstaff ---------------------------------------------------------------------------

Int Function FindRobotIndex(Actor akRobot)
	If akRobot == None || RobotData == None
		Return -1
	EndIf
	Int index = 0
	While index < RobotData.Length
		If RobotData[index].RobotAlias != None && RobotData[index].RobotAlias.GetActorReference() == akRobot
			Return index
		EndIf
		index += 1
	EndWhile
	Return -1
EndFunction

Function StartRobotPhase()
	bActivityStarted = True
	If RobotData == None
		Return
	EndIf
	Int index = 0
	While index < RobotData.Length
		Actor robot = None
		If RobotData[index].RobotAlias != None
			robot = RobotData[index].RobotAlias.GetActorReference()
		EndIf
		RobotData[index].RobotActor = robot
		If robot != None && !robot.IsDead()
			If RobotsPendingWork != None && RobotsPendingWork.Find(robot) < 0
				RobotsPendingWork.AddRef(robot)
			EndIf
			robot.EvaluatePackage()
		EndIf
		index += 1
	EndWhile
	PublishCounts()
	StartTimer(5.0, 13)
	StartTimer(TimeRunningOutTimerLength, TimeRunningOutTimerId)
EndFunction

Function SendRobotToWork(Actor akRobot)
	If SendRobotToWorkSpinLock || akRobot == None || akRobot.IsDead()
		Return
	EndIf
	If !IsActivityRunning() || IsStageDone(RobotStageDone) || RobotsPendingWork == None || RobotsPendingWork.Find(akRobot) < 0
		Return
	EndIf
	SendRobotToWorkSpinLock = True
	Int index = FindRobotIndex(akRobot)
	RobotsPendingWork.RemoveRef(akRobot)
	If RobotsSentToWork != None && RobotsSentToWork.Find(akRobot) < 0
		RobotsSentToWork.AddRef(akRobot)
	EndIf
	akRobot.EvaluatePackage()
	If index >= 0
		StartWave(RobotData[index].RobotEncounterWave)
	EndIf
	SendRobotToWorkSpinLock = False
	If akRobot.IsBleedingOut()
		RobotEnteredBleedout(akRobot)
	EndIf
EndFunction

Function RobotArrivedAtWork(Actor akRobot)
	If akRobot == None || akRobot.IsDead() || RobotsSentToWork == None || RobotsSentToWork.Find(akRobot) < 0
		Return
	EndIf
	RobotsSentToWork.RemoveRef(akRobot)
	If RobotsAtWork != None && RobotsAtWork.Find(akRobot) < 0
		RobotsAtWork.AddRef(akRobot)
	EndIf
	akRobot.EvaluatePackage()
	Int index = FindRobotIndex(akRobot)
	If index >= 0
		StopWave(RobotData[index].RobotEncounterWave)
	EndIf
	iRobotsWorkingCurrent += 1
	bRobotArrived = True
	StartSceneOnce(MTNM04_PA_RobotArrivedScene)
	PublishCounts()
	CheckRobotPhaseDone()
EndFunction

Function RobotEnteredBleedout(Actor akRobot)
	Int index = FindRobotIndex(akRobot)
	If index < 0 || !IsActivityRunning()
		Return
	EndIf
	Int objective = RobotData[index].BleedoutObjective
	If !IsObjectiveDisplayed(objective) || IsObjectiveCompleted(objective)
		SetObjectiveCompleted(objective, False)
		SetObjectiveDisplayed(objective, True, True)
	EndIf
EndFunction

; FO76 revives a downed waiter on interaction; no components are charged.
Function RepairRobot(Actor akRobot, Actor akRepairer)
	If akRobot == None || akRepairer == None || akRepairer != Game.GetPlayer() || akRobot.IsDead() || !akRobot.IsBleedingOut()
		Return
	EndIf
	Int index = FindRobotIndex(akRobot)
	If index < 0 || IsStageDone(231 + index)
		Return
	EndIf
	akRobot.ResetHealthAndLimbs()
	akRobot.EvaluatePackage()
	SetObjectiveDisplayed(RobotData[index].BleedoutObjective, False)
EndFunction

Function DestroyRobot(Int aiIndex)
	If RobotData == None || aiIndex < 0 || aiIndex >= RobotData.Length
		Return
	EndIf
	Int objective = RobotData[aiIndex].BleedoutObjective
	If IsObjectiveDisplayed(objective) && !IsObjectiveCompleted(objective)
		SetObjectiveFailed(objective, True)
	EndIf
	Actor robot = None
	If RobotData[aiIndex].RobotAlias != None
		robot = RobotData[aiIndex].RobotAlias.GetActorReference()
	EndIf
	If robot == None || robot.IsDead()
		Return
	EndIf
	Bool wasWorking = RobotsAtWork != None && RobotsAtWork.Find(robot) >= 0
	If RobotsPendingWork != None
		RobotsPendingWork.RemoveRef(robot)
	EndIf
	If RobotsSentToWork != None
		RobotsSentToWork.RemoveRef(robot)
	EndIf
	If RobotsAtWork != None
		RobotsAtWork.RemoveRef(robot)
	EndIf
	StopWave(RobotData[aiIndex].RobotEncounterWave)
	MTNM04_RobotsSentToWorkScript sentScript = RobotsSentToWork as MTNM04_RobotsSentToWorkScript
	If sentScript != None
		sentScript.ExplodeRobot(robot)
	EndIf
	robot.KillEssential()
	If wasWorking && iRobotsWorkingCurrent > 0
		iRobotsWorkingCurrent -= 1
	EndIf
	If iRobotsCurrentTotal > 0
		iRobotsCurrentTotal -= 1
	EndIf
	bRobotDestroyed = True
	StartSceneOnce(MTNM04_PA_RobotDeadScene)
	PublishCounts()
	CheckRobotPhaseDone()
EndFunction

Function CheckRobotPhaseDone()
	If !IsActivityRunning() || IsStageDone(RobotStageDone)
		Return
	EndIf
	Bool pending = RobotsPendingWork != None && RobotsPendingWork.GetCount() > 0
	Bool traveling = RobotsSentToWork != None && RobotsSentToWork.GetCount() > 0
	If !pending && !traveling
		SetStage(RobotStageDone)
	EndIf
EndFunction

; Trigger volumes can miss a walking actor, so arrival is also measured against the function room trigger.
Function CheckTravelingRobots()
	If RobotsSentToWork == None || RoomTrigger == None || RoomTrigger.GetReference() == None
		Return
	EndIf
	ObjectReference room = RoomTrigger.GetReference()
	Int index = RobotsSentToWork.GetCount() - 1
	While index >= 0
		Actor robot = RobotsSentToWork.GetAt(index) as Actor
		If robot != None && !robot.IsDead()
			If robot.GetDistance(room) < 600.0
				RobotArrivedAtWork(robot)
			ElseIf !robot.IsBleedingOut()
				robot.EvaluatePackage()
			EndIf
		EndIf
		index -= 1
	EndWhile
EndFunction

; ---- Place settings and centerpieces -----------------------------------------------------------

Function ScatterItems(RefCollectionAlias akMarkers, RefCollectionAlias akActivators, Activator akActivatorBase, Int aiCount)
	If akMarkers == None || akActivators == None || akActivatorBase == None || aiCount <= 0
		Return
	EndIf
	Int markerCount = akMarkers.GetCount()
	If markerCount <= 0
		Return
	EndIf
	If markerCount > 128
		markerCount = 128
	EndIf
	Int[] order = New Int[128]
	Int index = 0
	While index < markerCount
		order[index] = index
		index += 1
	EndWhile
	Int placed = 0
	index = 0
	While index < markerCount && placed < aiCount
		Int swapWith = Utility.RandomInt(index, markerCount - 1)
		Int chosen = order[swapWith]
		order[swapWith] = order[index]
		order[index] = chosen
		ObjectReference marker = akMarkers.GetAt(chosen)
		If marker != None
			ObjectReference item = marker.PlaceAtMe(akActivatorBase, 1, True, False, False)
			If item != None
				akActivators.AddRef(item)
				RegisterForRemoteEvent(item, "OnActivate")
				placed += 1
			EndIf
		EndIf
		index += 1
	EndWhile
EndFunction

Function StartTablePhase()
	CancelTimer(13)
	ScatterItems(PlacesettingMarkers, MisplacedPlacesettingActivators, MTNM04_Placesetting_Activator, iPlacesettingsStaticTotal)
	ScatterItems(CenterpieceMarkers, MisplacedCenterpieceActivators, MTNM04_Centerpiece_Activator, iCenterpiecesStaticTotal)
	PublishCounts()
	StartSceneOnce(MTNM04_PA_Phase2Scene)
	StartTimer(StartPhase2TimerLength as Float, StartPhase2ID)
EndFunction

Function StartUndesirableTracking()
	RefreshUndesirables()
	StartTimer(2.0, 9)
EndFunction

Function CollectMisplacedItem(ObjectReference akItem, Actor akCollector)
	If akItem == None || akCollector == None || akCollector != Game.GetPlayer()
		Return
	EndIf
	MiscObject item = None
	If MisplacedPlacesettingActivators != None && MisplacedPlacesettingActivators.Find(akItem) >= 0
		MisplacedPlacesettingActivators.RemoveRef(akItem)
		item = MTNM04_Placesetting
	ElseIf MisplacedCenterpieceActivators != None && MisplacedCenterpieceActivators.Find(akItem) >= 0
		MisplacedCenterpieceActivators.RemoveRef(akItem)
		item = MTNM04_Centerpiece
	Else
		Return
	EndIf
	UnregisterForRemoteEvent(akItem, "OnActivate")
	akItem.Disable(False)
	akItem.Delete()
	If item != None
		akCollector.AddItem(item, 1, False)
	EndIf
EndFunction

; Each table's four place settings sit within 50 units of it and the next table's start past 260, so 128 units selects only this table's slots.
; Each table's place settings sit within 50 units and its centerpiece within 20; the next table's start past 260.
ObjectReference Function FindOpenStatic(RefCollectionAlias akStatics, ObjectReference akTable)
	If akStatics == None || akTable == None
		Return None
	EndIf
	ObjectReference best = None
	Float bestDistance = 128.0
	Int index = 0
	Int count = akStatics.GetCount()
	While index < count
		ObjectReference candidate = akStatics.GetAt(index)
		If candidate != None && candidate.IsDisabled()
			Float distance = candidate.GetDistance(akTable)
			If distance < bestDistance
				best = candidate
				bestDistance = distance
			EndIf
		EndIf
		index += 1
	EndWhile
	Return best
EndFunction

Int Function PlaceItemsOnTable(ObjectReference akTable, Actor akPlayer, RefCollectionAlias akStatics, MiscObject akItem)
	If akItem == None
		Return 0
	EndIf
	Int placed = 0
	ObjectReference slot = FindOpenStatic(akStatics, akTable)
	While slot != None && akPlayer.GetItemCount(akItem) > 0
		akPlayer.RemoveItem(akItem, 1, True)
		slot.Enable(False)
		placed += 1
		ObjectReference filled = slot
		slot = FindOpenStatic(akStatics, akTable)
		If slot == filled
			slot = None
		EndIf
	EndWhile
	Return placed
EndFunction

Int Function SetTable(ObjectReference akTable, Actor akPlayer)
	If akTable == None || akPlayer == None || akPlayer != Game.GetPlayer()
		Return 0
	EndIf
	If !IsActivityRunning() || !IsStageDone(RobotStageDone) || IncrementPlacesettingsSpinLock
		Return 0
	EndIf
	IncrementPlacesettingsSpinLock = True
	Int settings = PlaceItemsOnTable(akTable, akPlayer, StaticPlacesettings, MTNM04_Placesetting)
	Int centerpieces = PlaceItemsOnTable(akTable, akPlayer, StaticCenterpieces, MTNM04_Centerpiece)
	iPlacesettingsCurrent += settings
	iCenterpiecesCurrent += centerpieces
	If FindOpenStatic(StaticPlacesettings, akTable) == None && FindOpenStatic(StaticCenterpieces, akTable) == None
		If FinishedTables != None && FinishedTables.Find(akTable) < 0
			FinishedTables.AddRef(akTable)
		EndIf
	EndIf
	IncrementPlacesettingsSpinLock = False
	If settings + centerpieces <= 0
		Return 0
	EndIf
	StartSceneOnce(MTNM04_PA_TableSetScene)
	PublishCounts()
	If iPlacesettingsCurrent >= iPlacesettingsStaticTotal && !IsStageDone(PlacesettingsDoneStage)
		SetStage(PlacesettingsDoneStage)
	EndIf
	If iCenterpiecesCurrent >= iCenterpiecesStaticTotal && !IsStageDone(CenterpiecesDoneStage)
		SetStage(CenterpiecesDoneStage)
	EndIf
	Return settings + centerpieces
EndFunction

; ---- Undesirables ------------------------------------------------------------------------------

RefCollectionAlias Function UndesirableCollection()
	MTNM04_AliasTriggerScript roomScript = RoomTrigger as MTNM04_AliasTriggerScript
	If roomScript == None
		Return None
	EndIf
	Return roomScript.UndesirablesInRoom
EndFunction

Function RefreshUndesirables()
	If UndesirableBreachedSpinLock
		Return
	EndIf
	UndesirableBreachedSpinLock = True
	RefCollectionAlias undesirables = UndesirableCollection()
	Int living = 0
	If undesirables != None
		Int index = undesirables.GetCount() - 1
		While index >= 0
			ObjectReference member = undesirables.GetAt(index)
			Actor memberActor = member as Actor
			If member == None || member.IsDisabled() || (memberActor != None && memberActor.IsDead())
				undesirables.RemoveRef(member)
			Else
				living += 1
			EndIf
			index -= 1
		EndWhile
	EndIf
	iUndesirablesBreached = living
	SetQuestVariable(UndesirablesInFunctionRoom, living)

	Bool active = IsActivityRunning() && IsStageDone(255)
	If living > 0 && active
		If !UndesirablesInRoom
			UndesirablesInRoom = True
			SetObjectiveDisplayed(UndesirabledBreachedObjective, True, True)
			StartSceneOnce(MTNM04_PA_UndesirableEnteredScene)
			StartTimer(UndesirablesBreachedWarningTimerLength as Float, UndesirablesBreachedWarningTimerId)
		EndIf
	ElseIf UndesirablesInRoom
		UndesirablesInRoom = False
		CancelTimer(UndesirablesBreachedWarningTimerId)
		CancelTimer(UndesirablesBreachedPenaltyTimerId)
		SetObjectiveDisplayed(UndesirabledBreachedObjective, False)
		If active
			StartSceneOnce(MTNM04_PA_UndesirablesClearedScene)
		EndIf
	EndIf
	UndesirableBreachedSpinLock = False
EndFunction

; Each uncleared interval costs one approval point; the gala still happens at the end.
Function ApplyUndesirablePenalty()
	RefreshUndesirables()
	If !UndesirablesInRoom || !IsActivityRunning()
		Return
	EndIf
	If iUnDesirablePenaltyCount < iUndesirablePenaltyMax
		iUnDesirablePenaltyCount += 1
		If iRewardPoints > 0
			iRewardPoints -= 1
		EndIf
	EndIf
	If iUnDesirablePenaltyCount >= iUndesirablePenaltyMax
		StartSceneOnce(MTNM04_PA_UndesirableFailScene)
	Else
		StartTimer(UndesirablesBreachedPenaltyTimerLength as Float, UndesirablesBreachedPenaltyTimerId)
	EndIf
EndFunction

; ---- Phases ------------------------------------------------------------------------------------

Function StartWavePhase(Int aiWaveStage)
	If !IsActivityRunning()
		Return
	EndIf
	If aiWaveStage == Wave1Stage
		StartWave(0)
		StartTimer(WaveTimerLength as Float, WaveTimer1Id)
	ElseIf aiWaveStage == BetweenWaves1_2Stage
		StartTimer(BetweenWavesTimerLength as Float, BetweenWaves1_2TimerId)
	ElseIf aiWaveStage == Wave2Stage
		StartWave(1)
		StartTimer(WaveTimerLength as Float, WaveTimer2Id)
	ElseIf aiWaveStage == BetweenWaves2_3Stage
		StartTimer(BetweenWavesTimerLength as Float, BetweenWaves2_3TimerId)
	ElseIf aiWaveStage == Wave3Stage
		StartWave(2)
	EndIf
EndFunction

Function StartEarlyCompletion()
	StartSceneOnce(MTNM04_PA_EarlyCompletionScene)
	DefaultQuestEncounterWaveScript waves = WaveScript()
	If waves != None
		StartWave(waves.FindEncounterWaveIndex("HardWaves"))
		StartWave(waves.FindEncounterWaveIndex("Boss01"))
	EndIf
EndFunction

Function EndActivity()
	Int timerID = 1
	While timerID <= 9
		CancelTimer(timerID)
		timerID += 1
	EndWhile
	CancelTimer(13)
	DeleteCollectionRefs(MisplacedPlacesettingActivators)
	DeleteCollectionRefs(MisplacedCenterpieceActivators)
	If UndesirablesInRoom
		UndesirablesInRoom = False
		SetObjectiveDisplayed(UndesirabledBreachedObjective, False)
	EndIf
	Int index = 0
	While RobotData != None && index < RobotData.Length
		SetObjectiveDisplayed(RobotData[index].BleedoutObjective, False)
		index += 1
	EndWhile
EndFunction

Function PlayEndScene()
	StartSceneOnce(MTNM04_PA_EndScene)
	B21ScenePollTicks = 0
	StartTimer(1.0, 11)
EndFunction

Function FinishGala()
	CancelTimer(11)
	If !IsRunning() || IsStageDone(SuccessStage) || IsStageDone(FailureStage) || IsStageDone(525)
		Return
	EndIf
	If iPlacesettingsCurrent + iCenterpiecesCurrent > 0 && iRewardPoints > 0
		SetStage(SuccessStage)
	Else
		SetStage(FailureStage)
	EndIf
EndFunction

Function ScheduleShutdown(Float afSeconds)
	CancelTimer(12)
	StartTimer(afSeconds, 12)
EndFunction

; ---- Events ------------------------------------------------------------------------------------

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
	Actor playerRef = Game.GetPlayer()
	If akActionRef != playerRef
		Return
	EndIf
	If Billingsley != None && akSender == Billingsley.GetReference()
		If IsRunning() && !IsStageDone(ActivityStartStage) && !IsStageDone(9990)
			SetStage(ActivityStartStage)
		EndIf
		Return
	EndIf
	CollectMisplacedItem(akSender, playerRef)
EndEvent

Event OnTimer(Int aiTimerID)
	If aiTimerID == BetweenWaves1_2TimerId
		If IsActivityRunning() && !IsStageDone(Wave2Stage)
			SetStage(Wave2Stage)
		EndIf
	ElseIf aiTimerID == BetweenWaves2_3TimerId
		If IsActivityRunning() && !IsStageDone(Wave3Stage)
			SetStage(Wave3Stage)
		EndIf
	ElseIf aiTimerID == StartPhase2ID
		If IsActivityRunning() && !IsStageDone(Wave1Stage)
			SetStage(Wave1Stage)
		EndIf
	ElseIf aiTimerID == UndesirablesBreachedWarningTimerId || aiTimerID == UndesirablesBreachedPenaltyTimerId
		ApplyUndesirablePenalty()
	ElseIf aiTimerID == TimeRunningOutTimerId
		If IsActivityRunning() && !IsStageDone(TimeRunningOutStage)
			StartSceneOnce(MTNM04_PA_TimeRunningOutScene)
			SetStage(TimeRunningOutStage)
		EndIf
	ElseIf aiTimerID == WaveTimer1Id
		If IsActivityRunning() && !IsStageDone(BetweenWaves1_2Stage)
			SetStage(BetweenWaves1_2Stage)
		EndIf
	ElseIf aiTimerID == WaveTimer2Id
		If IsActivityRunning() && !IsStageDone(BetweenWaves2_3Stage)
			SetStage(BetweenWaves2_3Stage)
		EndIf
	ElseIf aiTimerID == 9
		If IsActivityRunning()
			RefreshUndesirables()
			StartTimer(2.0, 9)
		EndIf
	ElseIf aiTimerID == 10
		B21ScenePollTicks += 1
		; The intro offers player dialogue; stop waiting after 90 s so a stalled scene cannot block the event.
		If MTNM04_Billingsley_IntroScene != None && MTNM04_Billingsley_IntroScene.IsPlaying() && B21ScenePollTicks < 90
			StartTimer(1.0, 10)
		Else
			FinishIntro()
		EndIf
	ElseIf aiTimerID == 11
		B21ScenePollTicks += 1
		If MTNM04_PA_EndScene != None && MTNM04_PA_EndScene.IsPlaying() && B21ScenePollTicks < 60
			StartTimer(1.0, 11)
		Else
			FinishGala()
		EndIf
	ElseIf aiTimerID == 12
		If IsRunning()
			Stop()
		EndIf
	ElseIf aiTimerID == 13
		If IsActivityRunning() && !IsStageDone(RobotStageDone)
			CheckTravelingRobots()
			CheckRobotPhaseDone()
			StartTimer(5.0, 13)
		EndIf
	EndIf
EndEvent

Event OnQuestShutdown()
	Int timerID = 1
	While timerID <= 13
		CancelTimer(timerID)
		timerID += 1
	EndWhile
	UnregisterForAllEvents()
	DeleteCollectionRefs(MisplacedPlacesettingActivators)
	DeleteCollectionRefs(MisplacedCenterpieceActivators)
	SetCollectionEnabled(StaticPlacesettings, False)
	SetCollectionEnabled(StaticCenterpieces, False)
	Actor playerRef = Game.GetPlayer()
	If playerRef != None
		If MTNM04_Placesetting != None && playerRef.GetItemCount(MTNM04_Placesetting) > 0
			playerRef.RemoveItem(MTNM04_Placesetting, playerRef.GetItemCount(MTNM04_Placesetting), True)
		EndIf
		If MTNM04_Centerpiece != None && playerRef.GetItemCount(MTNM04_Centerpiece) > 0
			playerRef.RemoveItem(MTNM04_Centerpiece, playerRef.GetItemCount(MTNM04_Centerpiece), True)
		EndIf
	EndIf
EndEvent
