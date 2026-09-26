Event OnQuestInit()
    Quest owner = Self as Quest
    EWS = owner as DefaultQuestEncounterWaveScript
    currentBossStage = 1
    lasersInitialised = False
    allLaserGrids = None
    activeTimers = New Int[0]
    B21StopStage = -1
    B21BossesReleased = False
    B21DefeatedBosses = New Actor[0]
    B21EntranceDoor = None
    RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
EndEvent

Event OnQuestShutdown()
    StopArenaTimers()
    CancelTimer(106)
    RestoreEntranceDoor()
    UnregisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
    activeTimers = New Int[0]
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender != Game.GetPlayer() || !IsRunning()
        Return
    EndIf
    If B21StopStage >= 0
        StartTimer(1.0, 106)
    ElseIf B21BossesReleased && !EventResolved()
        StartLaserGridCycle()
        If activeTimers != None && activeTimers.Length > 0
            StartTimer(2.0, 104)
        EndIf
    EndIf
EndEvent

Function CaptureEntranceDoor(ObjectReference akDoor)
    B21EntranceDoor = akDoor
    If akDoor != None
        B21DoorWasLocked = akDoor.IsLocked()
        B21DoorLockLevel = akDoor.GetLockLevel()
    EndIf
EndFunction

Function RestoreEntranceDoor()
    If B21EntranceDoor == None
        Return
    EndIf
    B21EntranceDoor.SetLockLevel(B21DoorLockLevel)
    If B21DoorWasLocked
        B21EntranceDoor.Lock(True)
    Else
        B21EntranceDoor.Unlock()
    EndIf
    B21EntranceDoor = None
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 104
        UpdateTimerCallouts()
    ElseIf aiTimerID == 105
        CycleLaserGrids()
    ElseIf aiTimerID == 106
        If B21StopStage >= 0 && IsRunning() && !IsStageDone(B21StopStage)
            SetStage(B21StopStage)
        EndIf
    EndIf
EndEvent

Bool Function EventResolved()
    Return IsStageDone(iCompletionStage) || IsStageDone(iFailureStage)
EndFunction

DefaultQuestEncounterWaveScript Function ResolveEncounterWaves()
    If EWS == None
        Quest owner = Self as Quest
        EWS = owner as DefaultQuestEncounterWaveScript
    EndIf
    Return EWS
EndFunction

Actor Function GetBossActor(Int aiIndex)
    If BossActors == None || aiIndex < 0 || aiIndex >= BossActors.Length || BossActors[aiIndex] == None
        Return None
    EndIf
    Return BossActors[aiIndex].GetActorReference()
EndFunction

Int Function CountLivingBosses(Actor akIgnoredBoss = None)
    Int living = 0
    Int index = 0
    While BossActors != None && index < BossActors.Length
        Actor boss = GetBossActor(index)
        If boss != None && boss != akIgnoredBoss && !boss.IsDead() && (B21DefeatedBosses == None || B21DefeatedBosses.Find(boss) < 0)
            living += 1
        EndIf
        index += 1
    EndWhile
    Return living
EndFunction

Bool Function ReleaseBosses()
    If BossActors == None || BossActors.Length != 3 || AllBosses == None
        Return False
    EndIf
    Int index = 0
    While index < BossActors.Length
        Actor boss = GetBossActor(index)
        If boss == None || boss.IsDead()
            Return False
        EndIf
        index += 1
    EndWhile
    AllBosses.RemoveAll()
    B21DefeatedBosses = New Actor[0]
    B21BossesReleased = False
    index = 0
    While index < BossActors.Length
        Actor boss = GetBossActor(index)
        AllBosses.AddRef(boss)
        If IncomingDamageState != None
            boss.SetValue(IncomingDamageState, 0.0)
        EndIf
        boss.Enable(False)
        Int polls = 0
        While !boss.Is3DLoaded() && polls < 20
            Utility.Wait(0.25)
            polls += 1
        EndWhile
        If !boss.Is3DLoaded() || !boss.PlayAnimation("AmbushExit")
            Return False
        EndIf
        boss.EvaluatePackage(False)
        index += 1
    EndWhile
    currentBossStage = 1
    B21BossesReleased = True
    Return True
EndFunction

Function HandleBossDeath(Actor akBoss)
    If !IsRunning() || EventResolved() || !B21BossesReleased || akBoss == None || BossActors == None
        Return
    EndIf
    If B21DefeatedBosses == None
        B21DefeatedBosses = New Actor[0]
    EndIf
    If B21DefeatedBosses.Find(akBoss) >= 0
        Return
    EndIf
    Int index = 0
    Bool knownBoss = False
    While index < BossActors.Length
        If GetBossActor(index) == akBoss
            knownBoss = True
        EndIf
        index += 1
    EndWhile
    If !knownBoss
        Return
    EndIf
    B21DefeatedBosses.Add(akBoss)
    Int remaining = BossActors.Length - B21DefeatedBosses.Length
    If remaining <= 2 && !IsStageDone(iBoss1CompleteStage)
        SetStage(iBoss1CompleteStage)
    EndIf
    If remaining <= 1 && !IsStageDone(iBoss2CompleteStage)
        SetStage(iBoss2CompleteStage)
    EndIf
    If remaining <= 0 && !IsStageDone(iBoss3CompleteStage)
        SetStage(iBoss3CompleteStage)
    EndIf
    currentBossStage = BossActors.Length - remaining + 1
EndFunction

Function EmpowerRemainingBosses(Float afDamageState)
    Int index = 0
    While BossActors != None && index < BossActors.Length
        Actor boss = GetBossActor(index)
        If boss != None && !boss.IsDead()
            If IncomingDamageState != None && boss.GetValue(IncomingDamageState) < afDamageState
                boss.SetValue(IncomingDamageState, afDamageState)
            EndIf
            If EpicRankUpFX_Spell != None
                EpicRankUpFX_Spell.Cast(boss, boss)
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Function SelfDestructBosses()
    Int index = 0
    While BossActors != None && index < BossActors.Length
        Actor boss = GetBossActor(index)
        If boss != None && !boss.IsDead()
            Quests:Storm:RegionBoss:StormBossAliasScript bossAlias = BossActors[index] as Quests:Storm:RegionBoss:StormBossAliasScript
            If bossAlias != None
                bossAlias.SelfDestruct()
            ElseIf RobotSelfDestructSpell != None
                boss.AddSpell(RobotSelfDestructSpell, False)
            EndIf
        EndIf
        index += 1
    EndWhile
EndFunction

Function StartMobWave(String asWaveID)
    If EventResolved()
        Return
    EndIf
    DefaultQuestEncounterWaveScript waves = ResolveEncounterWaves()
    If waves != None
        waves.StartEncounterWaveByID(asWaveID)
    EndIf
EndFunction

Function StartTimerCallouts()
    activeTimers = New Int[3]
    activeTimers[0] = i3mRemainTimerID
    activeTimers[1] = i1mRemainTimerID
    activeTimers[2] = i30sRemainTimerID
    StartTimer(2.0, 104)
EndFunction

Bool Function ClearPendingCallout(Int aiTimerID)
    If activeTimers == None
        Return False
    EndIf
    Int index = activeTimers.Find(aiTimerID)
    If index < 0
        Return False
    EndIf
    activeTimers.Remove(index)
    Return True
EndFunction

Function UpdateTimerCallouts()
    If activeTimers == None || activeTimers.Length == 0 || !IsRunning() || EventResolved()
        Return
    EndIf
    Quest owner = Self as Quest
    B21:QuestTimer questTimer = owner as B21:QuestTimer
    If questTimer == None
        Return
    EndIf
    If questTimer.IsQuestTimerRunning()
        Float remaining = questTimer.GetQuestTimerRemaining()
        Scene callout = None
        If remaining <= 180.0 && ClearPendingCallout(i3mRemainTimerID)
            callout = ThreeMinsRemainingScene
        EndIf
        If remaining <= 60.0 && ClearPendingCallout(i1mRemainTimerID)
            callout = OneMinRemainingScene
        EndIf
        If remaining <= 30.0 && ClearPendingCallout(i30sRemainTimerID)
            callout = ThirtySecRemainingScene
        EndIf
        If callout != None
            callout.Start()
        EndIf
    EndIf
    If activeTimers.Length > 0
        StartTimer(2.0, 104)
    EndIf
EndFunction

Function InitialiseLaserGrids()
    If lasersInitialised
        Return
    EndIf
    allLaserGrids = New Storm_LaserBarrierSyncScript[0]
    Int gridIndex = 0
    While Grids != None && gridIndex < Grids.Length
        ObjectReference gridParent = None
        If Grids[gridIndex] != None
            gridParent = Grids[gridIndex].GetReference()
        EndIf
        If gridParent != None
            ObjectReference[] children = gridParent.GetLinkedRefChildren(None)
            Int childIndex = 0
            While children != None && childIndex < children.Length
                Storm_LaserBarrierSyncScript grid = children[childIndex] as Storm_LaserBarrierSyncScript
                If grid != None && allLaserGrids.Find(grid) < 0
                    allLaserGrids.Add(grid)
                EndIf
                childIndex += 1
            EndWhile
        EndIf
        gridIndex += 1
    EndWhile
    lasersInitialised = True
EndFunction

Function StartLaserGridCycle()
    InitialiseLaserGrids()
    If allLaserGrids != None && allLaserGrids.Length > 0
        StartTimer(NextLaserGridDelay(), 105)
    EndIf
EndFunction

Float Function NextLaserGridDelay()
    Float minimum = fMinTimer
    Float maximum = fMaxTimer
    If maximum < minimum
        maximum = minimum
    EndIf
    If minimum < 1.0
        minimum = 1.0
    EndIf
    If maximum < minimum
        maximum = minimum
    EndIf
    Return Utility.RandomFloat(minimum, maximum)
EndFunction

Function CycleLaserGrids()
    If !IsRunning() || EventResolved() || allLaserGrids == None
        Return
    EndIf
    Int index = 0
    While index < allLaserGrids.Length
        If allLaserGrids[index] != None
            allLaserGrids[index].SetActivatorOpen(Utility.RandomInt(1, 100) <= iPercentChanceToOpen)
        EndIf
        index += 1
    EndWhile
    StartTimer(NextLaserGridDelay(), 105)
EndFunction

Function OpenLaserGrids()
    Int index = 0
    While allLaserGrids != None && index < allLaserGrids.Length
        If allLaserGrids[index] != None
            allLaserGrids[index].SetActivatorOpen(True)
        EndIf
        index += 1
    EndWhile
EndFunction

Function StopArenaTimers()
    CancelTimer(104)
    CancelTimer(105)
EndFunction

Function CleanupArena()
    StopArenaTimers()
    activeTimers = New Int[0]
    OpenLaserGrids()
    DefaultQuestEncounterWaveScript waves = ResolveEncounterWaves()
    If waves != None
        waves.StopAllEncounterWaves(True)
    EndIf
EndFunction

Function StopQuestTimer()
    Quest owner = Self as Quest
    B21:QuestTimer questTimer = owner as B21:QuestTimer
    If questTimer != None
        questTimer.StopQuestTimer()
    EndIf
EndFunction

Function ScheduleEventStop(Int aiStopStage, Float afDelay)
    B21StopStage = aiStopStage
    CancelTimer(106)
    StartTimer(afDelay, 106)
EndFunction
