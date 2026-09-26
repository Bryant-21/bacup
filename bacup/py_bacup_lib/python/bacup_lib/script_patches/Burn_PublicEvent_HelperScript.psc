Int Function StageTimerID()
    ; Offset the FO76 timer ids (0 and 1) away from ids other scripts on this quest may use.
    Return 71800 + iStageTimerID
EndFunction

Int Function HealthCheckTimerID()
    Return 71800 + DeathclawHealthCheckTimerID
EndFunction

Int Function ShutdownTimerID()
    Return 71899
EndFunction

Bool Function IsCleanupStageDone()
    Int index = 0
    While StagesToCleanUp != None && index < StagesToCleanUp.Length
        If IsStageDone(StagesToCleanUp[index])
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

Bool Function IsEventFinished()
    Return bStopDeathclawTimer || IsCleanupStageDone()
EndFunction

Actor Function GetTamedDeathclaw()
    If TamedDeathclaw == None
        Return None
    EndIf
    Return TamedDeathclaw.GetActorReference()
EndFunction

Function BeginEvent()
    ArmEmoteListener()
    RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
    b25DCDone = False
    b50DCDone = False
    b75DCDone = False
    bStopDeathclawTimer = False
    bRespawnedOnce = False
    B21PendingTimerStage = 0
    CancelTimer(StageTimerID())
    CancelTimer(ShutdownTimerID())
    EnsureTamedDeathclaw()
    CancelTimer(HealthCheckTimerID())
    StartTimer(HealthCheckInterval(), HealthCheckTimerID())
EndFunction

B21:B21_TFA_PublicEventController Function EmoteBus()
    Return Game.GetFormFromFile(0xFFF017, "B21_TalesFromAppalachia.esm") as B21:B21_TFA_PublicEventController
EndFunction

Function ArmEmoteListener()
    B21:B21_TFA_PublicEventController bus = EmoteBus()
    If bus != None
        RegisterForCustomEvent(bus, "B21EmoteV1")
    EndIf
EndFunction

Event Actor.OnPlayerLoadGame(Actor akSender)
    If IsRunning() && !IsEventFinished()
        ArmEmoteListener()
    EndIf
EndEvent

Event B21:B21_TFA_PublicEventController.B21EmoteV1(B21:B21_TFA_PublicEventController akSender, Var[] akArgs)
    If akArgs.Length != 5
        Return
    EndIf
    Int aiVersion = akArgs[0] as Int
    Actor akPlayer = akArgs[1] as Actor
    String asPlugin = akArgs[2] as String
    Int aiSourceID = akArgs[3] as Int
    Int aiCategoryID = akArgs[4] as Int
    If aiVersion != 1 || akPlayer != Game.GetPlayer() || asPlugin != "SeventySix.esm" || aiCategoryID != 22337 || (aiSourceID != 1114476 && aiSourceID != 5234353)
        Return
    EndIf
    If IsRunning() && !IsEventFinished() && IsStageDone(420) && !IsStageDone(470) && IsObjectiveDisplayed(32) && !IsObjectiveCompleted(32) && PlayersRefCol != None && PlayersRefCol.Find(akPlayer) >= 0
        SetStage(470)
    EndIf
EndEvent

Float Function HealthCheckInterval()
    If DeathclawTimerFrequencey < 1.0
        Return 1.0
    EndIf
    Return DeathclawTimerFrequencey
EndFunction

Actor Function EnsureTamedDeathclaw()
    Actor deathclaw = GetTamedDeathclaw()
    If deathclaw == None
        ; Alias_TamedDeathclaw names an unplaced actor with no create-at alias, so FO4 leaves it empty.
        ObjectReference spawnMarker = None
        If TamedDeathclawSpawnLoc != None
            spawnMarker = TamedDeathclawSpawnLoc.GetReference()
        EndIf
        If spawnMarker == None || TamedDeathclawForm == None || TamedDeathclaw == None
            Return None
        EndIf
        deathclaw = spawnMarker.PlaceAtMe(TamedDeathclawForm, 1, True, False, False) as Actor
        If deathclaw == None
            Return None
        EndIf
        TamedDeathclaw.ForceRefTo(deathclaw)
    EndIf
    RegisterForRemoteEvent(deathclaw, "OnDeath")
    If IsStageDone(iArmorUpDoneStage)
        ArmorUpDeathclaw()
    ElseIf FurnitureEnableValue != None
        ; Burn_E01_AmbushPackageTamedDC holds the Deathclaw in its cage while this value is 1.
        deathclaw.SetValue(FurnitureEnableValue, 1.0)
    EndIf
    deathclaw.EvaluatePackage()
    Return deathclaw
EndFunction

Function ArmorUpDeathclaw()
    Actor deathclaw = GetTamedDeathclaw()
    If deathclaw != None && DeathclawArmor != None
        deathclaw.SetOutfit(DeathclawArmor)
    EndIf
EndFunction

Function ReleaseDeathclaw()
    Actor deathclaw = GetTamedDeathclaw()
    If deathclaw == None
        Return
    EndIf
    If FurnitureEnableValue != None
        deathclaw.SetValue(FurnitureEnableValue, 0.0)
    EndIf
    deathclaw.EvaluatePackage()
EndFunction

Function StartStageTimer(Float afSeconds, Int aiStage)
    If aiStage <= 0 || IsStageDone(aiStage) || IsEventFinished()
        Return
    EndIf
    CancelTimer(StageTimerID())
    B21PendingTimerStage = aiStage
    If afSeconds < 1.0
        afSeconds = 1.0
    EndIf
    StartTimer(afSeconds, StageTimerID())
EndFunction

Function StartArmorUpTimer(Float afSeconds)
    StartStageTimer(afSeconds, iArmorUpDoneStage)
EndFunction

Function StartEscortTimer(Float afSeconds)
    StartStageTimer(afSeconds, iEscortDoneStage)
EndFunction

Function SetPendingTimerStage()
    Int stage = B21PendingTimerStage
    B21PendingTimerStage = 0
    If stage <= 0 || !IsRunning() || IsEventFinished() || IsStageDone(stage)
        Return
    EndIf
    SetStage(stage)
EndFunction

Function CheckDeathclaw()
    If !IsRunning() || IsEventFinished()
        Return
    EndIf

    Actor playerRef = Game.GetPlayer()
    If !IsStageDone(StartTrackingStage) && PlayersRefCol != None && playerRef != None && PlayersRefCol.Find(playerRef) >= 0
        SetStage(StartTrackingStage)
    EndIf

    Actor deathclaw = GetTamedDeathclaw()
    If deathclaw == None
        If !bRespawnedOnce && !IsStageDone(iEscortDoneStage)
            bRespawnedOnce = True
            EnsureTamedDeathclaw()
        EndIf
        Return
    EndIf
    If deathclaw.IsDead()
        FailForDeadDeathclaw()
        Return
    EndIf
    If !IsStageDone(iEscortDoneStage)
        Return
    EndIf

    Float health = deathclaw.GetValuePercentage(Game.GetHealthAV())
    If health <= 0.25 && !b25DCDone
        b25DCDone = True
        b50DCDone = True
        b75DCDone = True
        StartHealthScene(Health25Scene)
    ElseIf health <= 0.5 && !b50DCDone
        b50DCDone = True
        b75DCDone = True
        StartHealthScene(Health50Scene)
    ElseIf health <= 0.75 && !b75DCDone
        b75DCDone = True
        StartHealthScene(Health75Scene)
    EndIf
EndFunction

Function StartHealthScene(Scene akScene)
    If akScene != None && !akScene.IsPlaying()
        akScene.Start()
    EndIf
EndFunction

Function FailForDeadDeathclaw()
    If IsRunning() && !IsEventFinished() && TamedDCDeathFailStage > 0 && !IsStageDone(TamedDCDeathFailStage)
        SetStage(TamedDCDeathFailStage)
    EndIf
EndFunction

Function RemoveScrapFromPlayer()
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || ScrapForm == None
        Return
    EndIf
    Int count = playerRef.GetItemCount(ScrapForm)
    If count > 0
        playerRef.RemoveItem(ScrapForm, count, True)
    EndIf
EndFunction

Function StopDeathclawTracking()
    bStopDeathclawTimer = True
    B21PendingTimerStage = 0
    CancelTimer(HealthCheckTimerID())
    CancelTimer(StageTimerID())
    Actor deathclaw = GetTamedDeathclaw()
    If deathclaw != None
        UnregisterForRemoteEvent(deathclaw, "OnDeath")
    EndIf
EndFunction

Function CloseEvent(Scene akResultScene)
    B21:B21_TFA_PublicEventController bus = EmoteBus()
    If bus != None
        UnregisterForCustomEvent(bus, "B21EmoteV1")
    EndIf
    UnregisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
    StopDeathclawTracking()
    RemoveScrapFromPlayer()
    If akResultScene == None
        Stop()
        Return
    EndIf
    RegisterForRemoteEvent(akResultScene, "OnEnd")
    akResultScene.Start()
    ; Upper bound for the one-line result scene: a scene whose speaker cannot fill never sends OnEnd.
    StartTimer(20.0, ShutdownTimerID())
EndFunction

Function RemoveTamedDeathclaw()
    Actor deathclaw = GetTamedDeathclaw()
    If TamedDeathclaw != None
        TamedDeathclaw.Clear()
    EndIf
    If deathclaw != None
        UnregisterForRemoteEvent(deathclaw, "OnDeath")
        deathclaw.Delete()
    EndIf
EndFunction

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == iArmorUpDoneStage
        ArmorUpDeathclaw()
    ElseIf auiStageID == iPostRaiderStage || auiStageID == TamedDCFinalReposStage
        Actor deathclaw = GetTamedDeathclaw()
        If deathclaw != None
            deathclaw.EvaluatePackage()
        EndIf
    EndIf
    If StagesToCleanUp != None && StagesToCleanUp.Find(auiStageID) >= 0
        StopDeathclawTracking()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == HealthCheckTimerID()
        CheckDeathclaw()
        If IsRunning() && !IsEventFinished()
            StartTimer(HealthCheckInterval(), HealthCheckTimerID())
        EndIf
    ElseIf aiTimerID == StageTimerID()
        SetPendingTimerStage()
    ElseIf aiTimerID == ShutdownTimerID()
        If IsRunning() && bStopDeathclawTimer
            Stop()
        EndIf
    EndIf
EndEvent

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    If akSender != None && akSender == GetTamedDeathclaw()
        FailForDeadDeathclaw()
    EndIf
EndEvent

Event Scene.OnEnd(Scene akSender)
    UnregisterForRemoteEvent(akSender, "OnEnd")
    ; Registrations survive a restart, so only a finished run may stop the quest.
    If IsRunning() && bStopDeathclawTimer
        CancelTimer(ShutdownTimerID())
        Stop()
    EndIf
EndEvent

Event OnQuestShutdown()
    B21:B21_TFA_PublicEventController bus = EmoteBus()
    If bus != None
        UnregisterForCustomEvent(bus, "B21EmoteV1")
    EndIf
    UnregisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
    StopDeathclawTracking()
    CancelTimer(ShutdownTimerID())
    RemoveScrapFromPlayer()
    RemoveTamedDeathclaw()
EndEvent
