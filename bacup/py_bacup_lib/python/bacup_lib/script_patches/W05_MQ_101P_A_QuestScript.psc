Event OnQuestInit()
    RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
EndEvent

Event OnQuestShutdown()
    Actor playerRef = Game.GetPlayer()
    UnregisterForRemoteEvent(playerRef, "OnPlayerLoadGame")
    UnregisterForRemoteEvent(playerRef, "OnCombatStateChanged")
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 500
        ArmStageFallback(600, 60.0)
    ElseIf auiStageID == 960
        ArmStageFallback(970, 90.0)
    ElseIf auiStageID == 970
        ArmStageFallback(1000, 60.0)
    ElseIf auiStageID == 1050
        CheckMezzanineHostiles()
    ElseIf auiStageID == iStageSpawnMeg
        SpawnMegParty()
    ElseIf auiStageID == iStageAldridgeDead
        If IsStageDone(1460) && !IsStageDone(iStagePlayerLied)
            SetStage(iStagePlayerLied)
        EndIf
    ElseIf auiStageID == iStagePlayerPromised
        Actor playerRef = MQ101APlayer.GetActorReference()
        If playerRef && W05_MQ_101P_A_AldridgePromiseValue
            playerRef.SetValue(W05_MQ_101P_A_AldridgePromiseValue, 1.0)
        EndIf
    ElseIf auiStageID == 9000
        GrantCraterVendorPerk()
    EndIf
EndEvent

; Rose's radio lines and David's relay scene set 600/970/1000 natively when
; their last line ends. Remote Say on an unloaded speaker is unverified in
; FO4, so a timer keeps the quest moving if the native producer never fires.
Function ArmStageFallback(Int aiStage, Float afSeconds)
    If !IsStageDone(aiStage)
        StartTimer(afSeconds, aiStage)
    EndIf
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != 600 && aiTimerID != 970 && aiTimerID != 1000
        Return
    EndIf
    If IsRunning() && !IsStageDone(aiTimerID)
        Debug.Trace("W05_MQ_101P_A_QuestScript: broadcast producer for stage " + aiTimerID as String + " did not fire; advancing by fallback timer")
        SetStage(aiTimerID)
    EndIf
EndEvent

Function CheckMezzanineHostiles()
    If !IsStageDone(1050) || IsStageDone(iStageSpawnMeg)
        Return
    EndIf
    Int livingHostiles = 0
    Int index = 0
    While index < MezzanineHostiles.GetCount()
        Actor hostileRef = MezzanineHostiles.GetAt(index) as Actor
        If hostileRef && !hostileRef.IsDead()
            livingHostiles += 1
            RegisterForRemoteEvent(hostileRef, "OnDeath")
        EndIf
        index += 1
    EndWhile
    If livingHostiles > 0
        Return
    EndIf
    Actor playerRef = MQ101APlayer.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef
        If playerRef.IsInCombat()
            RegisterForRemoteEvent(playerRef, "OnCombatStateChanged")
            Return
        EndIf
        UnregisterForRemoteEvent(playerRef, "OnCombatStateChanged")
    EndIf
    SetStage(iStageSpawnMeg)
EndFunction

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    CheckMezzanineHostiles()
EndEvent

Event Actor.OnCombatStateChanged(Actor akSender, Actor akTarget, Int aeCombatState)
    If aeCombatState == 0
        CheckMezzanineHostiles()
    EndIf
EndEvent

Function SpawnMegParty()
    Actor megRef = MegAtToTW.GetActorReference()
    Actor raiderARef = RaiderAAtToTW.GetActorReference()
    Actor raiderBRef = RaiderBAtToTW.GetActorReference()
    If megRef
        megRef.Enable()
        megRef.EvaluatePackage()
    EndIf
    If raiderARef
        raiderARef.Enable()
        raiderARef.EvaluatePackage()
    EndIf
    If raiderBRef
        raiderBRef.Enable()
        raiderBRef.EvaluatePackage()
    EndIf
    WatchForMegArrival()
EndFunction

Function WatchForMegArrival()
    If !IsStageDone(iStageSpawnMeg) || IsStageDone(1110)
        Return
    EndIf
    Actor megRef = MegAtToTW.GetActorReference()
    ObjectReference playerRef = MQ101APlayer.GetReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If megRef && playerRef
        RegisterForDistanceLessThanEvent(megRef, playerRef, 512.0)
    EndIf
EndFunction

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    If IsStageDone(iStageSpawnMeg) && !IsStageDone(1110)
        SetStage(1110)
    EndIf
EndEvent

; Scene 0960 speaks through RelayTowerSpeaker, which has no fill rule. Alias 32
; creates the David-voiced talking activator next to Rose; alias 40 is the
; relay terminal the player used.
Function FillRelayTowerSpeaker()
    If RelayTowerSpeaker == None || RelayTowerSpeaker.GetReference() != None
        Return
    EndIf
    ObjectReference davidRef = None
    ReferenceAlias davidAlias = GetAlias(32) as ReferenceAlias
    If davidAlias
        davidRef = davidAlias.GetReference()
    EndIf
    If davidRef == None
        Return
    EndIf
    ObjectReference anchorRef = None
    ReferenceAlias terminalAlias = GetAlias(40) as ReferenceAlias
    If terminalAlias
        anchorRef = terminalAlias.GetReference()
    EndIf
    If anchorRef == None
        anchorRef = MQ101APlayer.GetReference()
    EndIf
    If anchorRef == None
        anchorRef = Game.GetPlayer()
    EndIf
    ObjectReference speakerRef = anchorRef.PlaceAtMe(davidRef.GetBaseObject(), 1, True)
    If speakerRef
        RelayTowerSpeaker.ForceRefTo(speakerRef)
    EndIf
EndFunction

Function GrantCraterVendorPerk()
    Actor playerRef = MQ101APlayer.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef && W05_Crater_PlayerVendorInteractChoicePerk && !playerRef.HasPerk(W05_Crater_PlayerVendorInteractChoicePerk)
        playerRef.AddPerk(W05_Crater_PlayerVendorInteractChoicePerk)
    EndIf
EndFunction

Event Actor.OnPlayerLoadGame(Actor akSender)
    If !IsRunning()
        Return
    EndIf
    If IsStageDone(1050) && !IsStageDone(iStageSpawnMeg)
        CheckMezzanineHostiles()
    EndIf
    WatchForMegArrival()
EndEvent
