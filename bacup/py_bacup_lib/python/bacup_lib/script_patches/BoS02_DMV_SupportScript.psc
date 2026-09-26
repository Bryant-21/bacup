Event OnQuestInit()
    B21DMVActors = new Actor[7]
    B21DMVSpawned = new Bool[7]
    B21DMVSpawning = False
    RegisterDMVEvents()
    ReconcileDMV()
EndEvent

Event OnQuestShutdown()
    CancelTimer(1)
    UnregisterForAllRemoteEvents()
    If pBoS02_DMV_Support_100_FirstLoop != None && pBoS02_DMV_Support_100_FirstLoop.IsPlaying()
        pBoS02_DMV_Support_100_FirstLoop.Stop()
    EndIf
    If pBoS02_DMV_Support_200_SecondLoop != None && pBoS02_DMV_Support_200_SecondLoop.IsPlaying()
        pBoS02_DMV_Support_200_SecondLoop.Stop()
    EndIf
    Int index = 0
    While B21DMVActors != None && index < B21DMVActors.Length
        Actor combatant = B21DMVActors[index]
        If combatant != None && !combatant.IsDead()
            combatant.DisableNoWait()
            combatant.Delete()
        EndIf
        index += 1
    EndWhile
    B21DMVActors = None
    B21DMVSpawned = None
EndEvent

Function RegisterDMVEvents()
    Actor playerRef = Game.GetPlayer()
    If playerRef != None
        RegisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    EndIf
    Quest mainQuest = Game.GetFormFromFile(0x0004E89C, "SeventySix.esm") as Quest
    If mainQuest != None
        RegisterForRemoteEvent(mainQuest, "OnStageSet")
    EndIf
    If pBoS02_DMV_Support_200_SecondLoop != None
        RegisterForRemoteEvent(pBoS02_DMV_Support_200_SecondLoop, "OnPhaseBegin")
    EndIf
EndFunction

Function BeginDepartment(Bool abDepartmentC)
    If !IsRunning()
        Return
    EndIf
    If abDepartmentC
        If pBoS02_DMVNumber_A3 != None && pBoS02_DMVNumber_A3.GetValue() > 0.0
            Return
        EndIf
        If pBoS02_DeptCCooldown != None
            pBoS02_DeptCCooldown.SetValue(0.0)
        EndIf
        SpawnDMVActors(3, 6)
        If pBoS02_DMV_Support_200_SecondLoop != None && !pBoS02_DMV_Support_200_SecondLoop.IsPlaying()
            pBoS02_DMV_Support_200_SecondLoop.Start()
        EndIf
    Else
        If pBoS02_DMVNumber_42 != None && pBoS02_DMVNumber_42.GetValue() > 0.0
            Return
        EndIf
        If pBoS02_DeptBCooldown != None
            pBoS02_DeptBCooldown.SetValue(0.0)
        EndIf
        SpawnDMVActors(0, 3)
        If pBoS02_DMV_Support_100_FirstLoop != None && !pBoS02_DMV_Support_100_FirstLoop.IsPlaying()
            pBoS02_DMV_Support_100_FirstLoop.Start()
        EndIf
    EndIf
EndFunction

Function SpawnDMVActors(Int aiFirst, Int aiEnd)
    If B21DMVSpawning || !IsRunning()
        Return
    EndIf
    ReferenceAlias spawnAlias = GetAlias(1) as ReferenceAlias
    ObjectReference spawnRef
    If spawnAlias != None
        spawnRef = spawnAlias.GetReference()
    EndIf
    ActorBase ghoulBase = Game.GetFormFromFile(0x00075337, "Fallout4.esm") as ActorBase
    Actor playerRef = Game.GetPlayer()
    If spawnRef == None || ghoulBase == None || playerRef == None
        StartTimer(5.0, 1)
        Return
    EndIf
    B21DMVSpawning = True
    If B21DMVActors == None
        B21DMVActors = new Actor[7]
    EndIf
    If B21DMVSpawned == None
        B21DMVSpawned = new Bool[7]
    EndIf
    RefCollectionAlias bossCollection = GetAlias(3) as RefCollectionAlias
    Int index = aiFirst
    While index < aiEnd && IsRunning()
        If !B21DMVSpawned[index]
            If index == 6 && bossCollection != None && bossCollection.GetCount() > 0
                B21DMVActors[index] = bossCollection.GetAt(0) as Actor
                B21DMVSpawned[index] = True
            EndIf
        EndIf
        If !B21DMVSpawned[index]
            Actor combatant = spawnRef.PlaceAtMe(ghoulBase, 1, False, True) as Actor
            B21DMVActors[index] = combatant
            If combatant != None
                B21DMVSpawned[index] = True
                If index == 6
                    If bossCollection != None
                        bossCollection.AddRef(combatant)
                        BoS02J47Script bossScript = bossCollection as BoS02J47Script
                        If bossScript != None
                            bossScript.ReconcileJ47()
                        EndIf
                    EndIf
                EndIf
                combatant.EnableNoWait()
                combatant.StartCombat(playerRef)
            Else
                StartTimer(5.0, 1)
            EndIf
        EndIf
        index += 1
    EndWhile
    B21DMVSpawning = False
EndFunction

Function ReconcileDMV()
    Quest mainQuest = Game.GetFormFromFile(0x0004E89C, "SeventySix.esm") as Quest
    If !IsRunning() || mainQuest == None
        Return
    EndIf
    Int stage = mainQuest.GetStage()
    If stage >= 1400 || !mainQuest.IsRunning()
        Stop()
    ElseIf stage >= 1300
        BeginDepartment(True)
        If (pBoS02_DMV_Support_200_SecondLoop != None && pBoS02_DMV_Support_200_SecondLoop.IsActionComplete(2)) || (pBoS02_DMVNumber_A3 != None && pBoS02_DMVNumber_A3.GetValue() > 0.0)
            SpawnDMVActors(6, 7)
        EndIf
    ElseIf stage >= 650 && stage < 800
        BeginDepartment(False)
    EndIf
EndFunction

Event Scene.OnPhaseBegin(Scene akSender, Int auiPhaseIndex)
    If akSender == pBoS02_DMV_Support_200_SecondLoop && auiPhaseIndex == 2
        SpawnDMVActors(6, 7)
    EndIf
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If akSender == (Game.GetFormFromFile(0x0004E89C, "SeventySix.esm") as Quest)
        ReconcileDMV()
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        RegisterDMVEvents()
        ReconcileDMV()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1
        ReconcileDMV()
    EndIf
EndEvent
