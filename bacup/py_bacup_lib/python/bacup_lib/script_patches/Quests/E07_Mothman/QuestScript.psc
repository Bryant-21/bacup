; FO76's server paced this event. Pacing here: the vines, tasks and pyres advance on their own counts, each
; defense phase waits a prep downtime, runs its EncounterWaves rows and ends on their WaveComplete events.
; Wave rows: 0-3 albino radstags, 4-5 church, 6-7 bridge, 8-9 waterside, 10 waterside special spawn.

Function StartEvent()
    Num_BloodAdded = 0
    Num_DancesDanced = 0
    Num_BossesKilled = 0
    Num_TotemsDestroyed = 0
    Num_TasksCompleted = 0
    SetGlobal(E07A_Mothman_Global_EventState, 1.0)
    SetGlobal(E07A_Mothman_Global_Num_PyresLit, 0.0)
    SetGlobal(E07A_Mothman_Global_BonfireTasks_Completed, 0.0)
    SetGlobal(E07A_Mothman_Global_Barricade_Church, 0.0)
    SetGlobal(E07A_Mothman_Global_Barricade_Bridge, 0.0)
    SetGlobal(E07A_Mothman_Global_Barricade_Waterside, 0.0)
    SetDisplayVariable("Pyres_Current", 0)
    SetDisplayVariable("Bosses_Current", 0)
    SetDisplayVariable("Totems_Current", 0)
    SetDisplayVariable("Blood_Current", 0)
    SetDisplayVariable("pyrelights_Current", 0)
    SetDisplayVariable("Dances_Current", 0)
    RepairCollection(GetAlias(91) as RefCollectionAlias)
    RepairCollection(GetAlias(95) as RefCollectionAlias)
    RepairCollection(GetAlias(94) as RefCollectionAlias)
    RepairCollection(GetAlias(97) as RefCollectionAlias)
    ResetPyre(109)
    ResetPyre(110)
    ResetPyre(111)

    DefaultQuestEncounterWaveScript waves = Waves()
    If waves != None
        RegisterForCustomEvent(waves, "WaveComplete")
    EndIf
    ; Clarence's speech line sets 150 and starts the explanation scene; 160 is the end of that scene.
    Scene explanation = Game.GetFormFromFile(0x629A78, "SeventySix.esm") as Scene
    If explanation != None
        RegisterForRemoteEvent(explanation, "OnEnd")
    EndIf

    Quests:E07_Mothman:MasterScript master = DialogueMaster()
    If master != None
        master.EventStarted()
    EndIf
EndFunction

; The cultists' dialogue quest (E07A_Mothman_Dialogue) had a server start; it runs from the first Equinox on.
Quests:E07_Mothman:MasterScript Function DialogueMaster()
    Quest dialogue = Game.GetFormFromFile(0x6173A2, "SeventySix.esm") as Quest
    If dialogue == None
        Return None
    EndIf
    If !dialogue.IsRunning() && !dialogue.Start()
        Return None
    EndIf
    Return dialogue as Quests:E07_Mothman:MasterScript
EndFunction

Event OnQuestShutdown()
    CancelTimer(501)
    CancelTimer(502)
    CancelTimer(TimerID_SpawnTimer)
    Quests:E07_Mothman:MasterScript master = DialogueMaster()
    If master != None
        master.EventEnded(IsStageDone(Stage_Completion))
    Else
        SetGlobal(E07A_Mothman_Global_EventState, 0.0)
    EndIf
    If RitualWeather != None
        Weather.ReleaseOverride()
    EndIf
    DefaultQuestEncounterWaveScript waves = Waves()
    If waves != None
        UnregisterForCustomEvent(waves, "WaveComplete")
    EndIf
    ReferenceAlias wiseMothman = GetAlias(127) as ReferenceAlias
    If wiseMothman != None && wiseMothman.GetReference() != None
        UnregisterForRemoteEvent(wiseMothman.GetReference(), "OnActivate")
        If master == None || !IsStageDone(Stage_Completion)
            wiseMothman.GetReference().Disable()
        EndIf
    EndIf
EndEvent

Event Scene.OnEnd(Scene akSender)
    If IsStageDone(150) && !IsStageDone(160)
        SetStage(160)
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 160 || (auiStageID >= 210 && auiStageID <= 230)
        CheckBarricades()
    ElseIf auiStageID == 240
        SetStage(1000)
        SetStage(2000)
        SetStage(3000)
    ElseIf auiStageID == 1000
        SpawnCultistBosses()
    ElseIf auiStageID == 2000
        TotemDestroyed()
    ElseIf auiStageID == 3000
        StartRadstagHunt()
    ElseIf auiStageID == Stage_BossesComplete || auiStageID == Stage_TotemsComplete || auiStageID == Stage_BloodComplete
        TaskCompleted()
    ElseIf auiStageID >= 4100 && auiStageID <= 4300
        PyreLit()
    ElseIf auiStageID == Stage_WeatherHasStarted
        If RitualWeather != None
            RitualWeather.SetActive(True, True)
        EndIf
        StartTimer(10.0, 502)
    ElseIf auiStageID == 4600
        SetStage(5040)
    ElseIf auiStageID == 5040 || auiStageID == 5100 || auiStageID == 5200 || auiStageID == 5300
        StartTimer(30.0, 501)
    ElseIf auiStageID == 5055
        StartWaves(4, 5, -1)
    ElseIf auiStageID == 5155
        StartWaves(6, 7, -1)
    ElseIf auiStageID == 5251
        StartWaves(10, -1, -1)
    ElseIf auiStageID == 5255
        StartWaves(8, 9, -1)
    ElseIf auiStageID == 5355
        StartWaves(4, 6, 8)
    ElseIf auiStageID == 5500
        DefaultQuestEncounterWaveScript waves = Waves()
        If waves != None
            waves.StopAllEncounterWaves()
        EndIf
    ElseIf auiStageID == Stage_DancesComplete
        SummonWiseMothman()
    ElseIf auiStageID >= 9100 && auiStageID <= 9300
        If IsStageDone(9100) && IsStageDone(9200) && IsStageDone(9300) && !IsStageDone(Stage_Completion)
            SetStage(9997)
        EndIf
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 501
        ; Prep downtime between defense phases.
        If !IsStageDone(5050)
            SetStage(5050)
        ElseIf IsStageDone(5100) && !IsStageDone(5150)
            SetStage(5150)
        ElseIf IsStageDone(5200) && !IsStageDone(5250)
            SetStage(5250)
        ElseIf IsStageDone(5300) && !IsStageDone(5350)
            SetStage(5350)
        EndIf
    ElseIf aiTimerID == TimerID_SpawnTimer
        TagRadstagBlood()
        If IsStageDone(3000) && !IsStageDone(Stage_BloodComplete)
            StartTimer(3.0, TimerID_SpawnTimer)
        EndIf
    ElseIf aiTimerID == 502
        If IsStageDone(Stage_WeatherHasStarted) && !IsStageDone(4600)
            SetStage(4600)
        EndIf
    EndIf
EndEvent

Event DefaultQuestEncounterWaveScript.WaveComplete(DefaultQuestEncounterWaveScript akSender, Var[] akArgs)
    If IsStageDone(3000) && !IsStageDone(Stage_BloodComplete) && !RadstagWavesActive()
        ; Not enough blood yet; send the radstags back.
        StartRadstagHunt()
        Return
    EndIf
    If IsStageDone(5055) && !IsStageDone(5100)
        If WavesFinished(4, 5, -1)
            SetStage(5100)
        EndIf
    ElseIf IsStageDone(5155) && !IsStageDone(5200)
        If WavesFinished(6, 7, -1)
            SetStage(5200)
        EndIf
    ElseIf IsStageDone(5255) && !IsStageDone(5300)
        If WavesFinished(8, 9, 10)
            SetStage(5300)
        EndIf
    ElseIf IsStageDone(5355) && !IsStageDone(5400)
        If WavesFinished(4, 6, 8)
            SetStage(5400)
        EndIf
    EndIf
EndEvent

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer() && IsStageDone(Stage_DancesComplete) && !IsStageDone(Stage_Completion)
        UnregisterForRemoteEvent(akSender, "OnActivate")
        SetStage(Stage_Completion)
    EndIf
EndEvent

Function CheckBarricades()
    Int cleared = 0
    If IsStageDone(210)
        cleared += 1
    EndIf
    If IsStageDone(220)
        cleared += 1
    EndIf
    If IsStageDone(230)
        cleared += 1
    EndIf
    SetDisplayVariable("Pyres_Current", cleared)
    If cleared >= 3 && IsStageDone(160) && !IsStageDone(240)
        SetStage(240)
    EndIf
EndFunction

Function SpawnCultistBosses()
    If refcol_BossSpawnMarkers == None || refcol_CultistBosses == None || actor_CultistBoss == None || refcol_BossSpawnMarkers.GetCount() == 0
        Return
    EndIf
    Int spawned = 0
    While spawned < Goal_BossesKilled
        ObjectReference marker = refcol_BossSpawnMarkers.GetAt(spawned % refcol_BossSpawnMarkers.GetCount())
        If marker != None
            Actor boss = marker.PlaceActorAtMe(actor_CultistBoss)
            If boss != None
                refcol_CultistBosses.AddRef(boss)
            EndIf
        EndIf
        spawned += 1
    EndWhile
EndFunction

Function BossKilled()
    Num_BossesKilled += 1
    SetDisplayVariable("Bosses_Current", Num_BossesKilled)
    If Num_BossesKilled >= Goal_BossesKilled && IsStageDone(1000)
        SetStage(Stage_BossesComplete)
    EndIf
EndFunction

; Totems are destructible MSTTs; recounting the destroyed ones keeps a repeated destruction event from double counting.
Function TotemDestroyed()
    RefCollectionAlias totems = GetAlias(97) as RefCollectionAlias
    If totems == None
        Return
    EndIf
    Num_TotemsDestroyed = CountDestroyed(totems)
    SetDisplayVariable("Totems_Current", Num_TotemsDestroyed)
    Int goal = Goal_TotemsDestroyed
    If goal > totems.GetCount()
        goal = totems.GetCount()
    EndIf
    If Num_TotemsDestroyed >= goal && IsStageDone(2000)
        SetStage(Stage_TotemsComplete)
    EndIf
EndFunction

Function StartRadstagHunt()
    DefaultQuestEncounterWaveScript waves = Waves()
    If waves != None
        Int index = 0
        While index <= 3
            waves.StartEncounterWave(index)
            index += 1
        EndWhile
    EndIf
    StartTimer(3.0, TimerID_SpawnTimer)
EndFunction

Bool Function RadstagWavesActive()
    DefaultQuestEncounterWaveScript waves = Waves()
    If waves == None
        Return False
    EndIf
    Int index = 0
    While index <= 3
        If waves.IsEncounterWaveSpawning(index)
            Return True
        EndIf
        index += 1
    EndWhile
    Return waves.CountLivingCollectionActors(Alias_refcol_Enemies_AlbinoRadstags) > 0
EndFunction

; FO76's server placed the blood on albino radstag corpses; register each new radstag once and add it on death.
Function TagRadstagBlood()
    If Alias_refcol_Enemies_AlbinoRadstags == None
        Return
    EndIf
    Int index = 0
    While index < Alias_refcol_Enemies_AlbinoRadstags.GetCount()
        Actor radstag = Alias_refcol_Enemies_AlbinoRadstags.GetAt(index) as Actor
        If radstag != None && !radstag.IsDead()
            RegisterForRemoteEvent(radstag, "OnDeath")
        EndIf
        index += 1
    EndWhile
EndFunction

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    UnregisterForRemoteEvent(akSender, "OnDeath")
    MiscObject blood = BloodItem()
    If blood != None && IsStageDone(3000) && !IsStageDone(Stage_BloodComplete)
        akSender.AddItem(blood)
    EndIf
EndEvent

MiscObject Function BloodItem()
    Quests:E07_Mothman:DepositBoxScript troughs = GetAlias(102) as Quests:E07_Mothman:DepositBoxScript
    If troughs != None
        Return troughs.ItemToLookFor
    EndIf
    Return None
EndFunction

Function BloodAdded(Int aiCount)
    Num_BloodAdded += aiCount
    SetDisplayVariable("Blood_Current", Num_BloodAdded)
    If Num_BloodAdded >= Goal_BloodAdded && IsStageDone(3000)
        SetStage(Stage_BloodComplete)
    EndIf
EndFunction

Int Function BloodStillNeeded()
    Return Goal_BloodAdded - Num_BloodAdded
EndFunction

Function TaskCompleted()
    Num_TasksCompleted = 0
    If IsStageDone(Stage_BossesComplete)
        Num_TasksCompleted += 1
    EndIf
    If IsStageDone(Stage_TotemsComplete)
        Num_TasksCompleted += 1
    EndIf
    If IsStageDone(Stage_BloodComplete)
        Num_TasksCompleted += 1
    EndIf
    SetGlobal(E07A_Mothman_Global_BonfireTasks_Completed, Num_TasksCompleted)
    If Num_TasksCompleted >= 3 && !IsStageDone(Stage_AllTasksComplete)
        SetStage(Stage_AllTasksComplete)
    EndIf
EndFunction

Function PyreLit()
    Int lit = 0
    If IsStageDone(4100)
        lit += 1
    EndIf
    If IsStageDone(4200)
        lit += 1
    EndIf
    If IsStageDone(4300)
        lit += 1
    EndIf
    SetGlobal(E07A_Mothman_Global_Num_PyresLit, lit)
    SetDisplayVariable("pyrelights_Current", lit)
    If lit >= 3 && !IsStageDone(Stage_WeatherHasStarted)
        SetStage(Stage_WeatherHasStarted)
    EndIf
EndFunction

Function StartWaves(Int aiFirst, Int aiSecond, Int aiThird)
    DefaultQuestEncounterWaveScript waves = Waves()
    If waves == None
        Return
    EndIf
    waves.StartEncounterWave(aiFirst)
    If aiSecond >= 0
        waves.StartEncounterWave(aiSecond)
    EndIf
    If aiThird >= 0
        waves.StartEncounterWave(aiThird)
    EndIf
EndFunction

Bool Function WavesFinished(Int aiFirst, Int aiSecond, Int aiThird)
    Return WaveFinished(aiFirst) && WaveFinished(aiSecond) && WaveFinished(aiThird)
EndFunction

Bool Function WaveFinished(Int aiWave)
    If aiWave < 0
        Return True
    EndIf
    DefaultQuestEncounterWaveScript waves = Waves()
    If waves == None || waves.IsEncounterWaveSpawning(aiWave)
        Return False
    EndIf
    Return waves.CountLivingCollectionActors(waves.EncounterWaves[aiWave].WaveRefCollection) <= 0
EndFunction

Function Danced()
    If !IsStageDone(6000) || IsStageDone(Stage_DancesComplete)
        Return
    EndIf
    Num_DancesDanced += 1
    SetDisplayVariable("Dances_Current", Num_DancesDanced)
    If Num_DancesDanced >= Goal_DancesDanced
        SetStage(Stage_DancesComplete)
    EndIf
EndFunction

Function SummonWiseMothman()
    ReferenceAlias wiseMothman = GetAlias(127) as ReferenceAlias
    If wiseMothman == None || wiseMothman.GetReference() == None
        SetStage(Stage_Completion)
        Return
    EndIf
    ObjectReference mothmanRef = wiseMothman.GetReference()
    mothmanRef.Enable()
    RegisterForRemoteEvent(mothmanRef, "OnActivate")
EndFunction

Function RepairCollection(RefCollectionAlias akCollection)
    If akCollection == None
        Return
    EndIf
    Int index = 0
    While index < akCollection.GetCount()
        ObjectReference target = akCollection.GetAt(index)
        If target != None
            target.ClearDestruction()
            target.Enable()
        EndIf
        index += 1
    EndWhile
EndFunction

Function ResetPyre(Int aiAliasID)
    ReferenceAlias pyreAlias = GetAlias(aiAliasID) as ReferenceAlias
    If pyreAlias == None || pyreAlias.GetReference() == None
        Return
    EndIf
    ObjectReference pyre = pyreAlias.GetReference()
    pyre.ClearDestruction()
    B21TwoStateActivator76 twoState = pyre as B21TwoStateActivator76
    If twoState != None
        twoState.SetOpen(False)
    EndIf
EndFunction

Int Function CountDestroyed(RefCollectionAlias akCollection)
    Int destroyed = 0
    Int index = 0
    While index < akCollection.GetCount()
        ObjectReference target = akCollection.GetAt(index)
        If target != None && target.IsDestroyed()
            destroyed += 1
        EndIf
        index += 1
    EndWhile
    Return destroyed
EndFunction

DefaultQuestEncounterWaveScript Function Waves()
    Return (Self as Quest) as DefaultQuestEncounterWaveScript
EndFunction

Function SetGlobal(GlobalVariable akGlobal, Float afValue)
    If akGlobal != None
        akGlobal.SetValue(afValue)
    EndIf
EndFunction

Function SetDisplayVariable(String asName, Float afValue)
    B21:QuestVariables questVariables = (Self as Quest) as B21:QuestVariables
    If questVariables != None
        questVariables.SetVariable(asName, afValue)
    EndIf
EndFunction
