Bool Function PrepareLocalBlast(ObjectReference akBlastMarker, Actor akLaunchingPlayer, Int aiSiloID, Int aiLaunchID, Spell akSmokeSpell, Spell akBlastSpell)
    If akBlastMarker == None
        Return False
    EndIf
    SmokeSpell = akSmokeSpell
    BlastSpell = akBlastSpell
    CancelTimer(9)
    ClearLocalBlastArt()
    bNukeTriggered = False
    bNukeTouchdown = False
    bTriggedOnce = False
    bWrapUpActive = False
    iDebugValue = aiSiloID
    iDebugwithCountdownValue = aiLaunchID
    Return True
EndFunction

Bool Function IsLocalCountdownActive()
    Return bTriggedOnce && !bNukeTriggered && (Self as Quest).IsRunning()
EndFunction

Bool Function HasLocalDetonated()
    Return bNukeTouchdown
EndFunction

Bool Function RecoverUnarmedLocalBlast()
    Quest fleeQuest = Self as Quest
    If !fleeQuest.IsRunning() || fleeQuest.IsStageDone(100) || bTriggedOnce || bNukeTriggered
        Return False
    EndIf
    If iDebugValue < 0 || iDebugValue > 2 || iDebugwithCountdownValue != iDebugValue + 3
        Return False
    EndIf
    EN07_NukeMasterScript masterScript = EN07_MQ_Nuke_Master as EN07_NukeMasterScript
    If masterScript == None
        Return False
    EndIf
    fleeQuest.Stop()
    masterScript.ResetLocalSilo(iDebugValue, iDebugwithCountdownValue)
    fleeQuest.Reset()
    Debug.Trace("[B21 Nuke] Recovered an unarmed launch; no active countdown was canceled.")
    Return True
EndFunction

Bool Function BeginLocalBlast(ObjectReference akBlastMarker, Actor akLaunchingPlayer, Int aiSiloID, Int aiLaunchID, Spell akSmokeSpell, Spell akBlastSpell)
    Quest fleeQuest = Self as Quest
    If !fleeQuest.IsRunning() || akBlastMarker == None
        Return False
    EndIf
    NukeBlastMarker.ForceRefTo(akBlastMarker)
    If akLaunchingPlayer == None
        akLaunchingPlayer = Game.GetPlayer()
    EndIf
    LaunchingPlayer.ForceRefTo(akLaunchingPlayer)
    Location blastLocation = akBlastMarker.GetCurrentLocation()
    If blastLocation != None
        TriggerLocation.ForceLocationTo(blastLocation)
    EndIf
    If !fleeQuest.IsStageDone(10)
        fleeQuest.SetStage(10)
    EndIf
    ; The local caller must also work when the installed fragment is stale.
    BeginLocalCountdown()
    fleeQuest.SetActive(True)
    Debug.Trace("[B21 Nuke] Countdown armed=" + bTriggedOnce + " marker=" + NukeBlastMarker.GetReference())
    Return fleeQuest.IsRunning() && bTriggedOnce
EndFunction

Function HandleStage(Int aiStage)
    If aiStage == 10
        BeginLocalCountdown()
    ElseIf aiStage == 100
        DetonateLocalBlast()
    EndIf
EndFunction

Function BeginLocalCountdown()
    If bTriggedOnce
        Return
    EndIf
    ObjectReference blastMarker = NukeBlastMarker.GetReference()
    Actor player = LaunchingPlayer.GetReference() as Actor
    If player == None
        player = Game.GetPlayer()
    EndIf
    If blastMarker == None
        Debug.Trace("[B21 Nuke] Countdown deferred: blast alias is not filled.")
        Return
    EndIf
    bTriggedOnce = True

    EN07_NukeBlastMarkerRefScript markerScript = blastMarker as EN07_NukeBlastMarkerRefScript
    If markerScript != None
        markerScript.ClientUpdateMapHazards(True)
    Else
        EN07_NukeMapHazardFormlist.AddForm(blastMarker)
    EndIf
    ObjectReference warningMarker = AudioWarningMarker.GetReference()
    If warningMarker != None
        warningMarker.MoveTo(blastMarker)
        warningMarker.Enable()
    EndIf
    If EN07_ApplySoundCategorySpell != None
        EN07_ApplySoundCategorySpell.Cast(player, player)
    EndIf
    Quest fleeQuest = Self as Quest
    Var[] announceArgs = new Var[1]
    announceArgs[0] = fleeQuest
    Utility.CallGlobalFunction("B21_NukeZones", "AnnounceQuestStart", announceArgs)
    fleeQuest.SetObjectiveDisplayed(iFleeObjID, True, True)
    fleeQuest.SetActive(True)
    Debug.Trace("[B21 Nuke] Quest announcement objective=" + iFleeObjID + " displayed=" + fleeQuest.IsObjectiveDisplayed(iFleeObjID) + " running=" + fleeQuest.IsRunning())
    StartTimer(30.0, 2)
    Debug.Trace("[B21 Nuke] Countdown started: 30 seconds; objective=" + iFleeObjID)
    Actor warningVoice = NukeVoice.GetReference() as Actor
    Topic warningTopic = Game.GetFormFromFile(0x002D0F6D, "SeventySix.esm") as Topic
    If warningVoice != None && warningTopic != None
        warningVoice.Say(warningTopic, None, True, player)
    EndIf
    Debug.Trace("[B21 Nuke] Warning voice=" + warningVoice + " topic=" + warningTopic)
EndFunction

Bool Function IsPlayerInBlastWorld(ObjectReference akMarker)
    Actor player = Game.GetPlayer()
    Return akMarker != None && akMarker.GetWorldSpace() != None && !player.IsInInterior() && player.GetWorldSpace() == akMarker.GetWorldSpace()
EndFunction

Function BeginLocalDescent()
    If bWrapUpActive || bNukeTriggered
        Return
    EndIf
    ObjectReference blastMarker = NukeBlastMarker.GetReference()
    If blastMarker == None
        Return
    EndIf
    bWrapUpActive = True
    StartTimer(15.0, 8)
    Int arrivalPolls = 0
    While !bNukeTriggered && !IsPlayerInBlastWorld(blastMarker) && arrivalPolls < 60
        Utility.Wait(0.25)
        arrivalPolls += 1
    EndWhile
    If bNukeTriggered || !IsPlayerInBlastWorld(blastMarker)
        Return
    EndIf
    ObjectReference missile = IncomingNuke.GetReference()
    If missile != None
        missile.MoveTo(blastMarker)
        Var[] skyArgs = new Var[2]
        skyArgs[0] = missile
        skyArgs[1] = blastMarker.GetWorldSpace()
        Utility.CallGlobalFunction("B21_NukeZones", "PrepareCloud", skyArgs)
        missile.EnableNoWait()
        Var[] args = new Var[2]
        args[0] = missile
        args[1] = arrivalPolls == 0
        CallFunctionNoWait("AnimateLocalMissile", args)
    EndIf
    Debug.Trace("[B21 Nuke] Warhead loading: missile=" + missile + " target=" + blastMarker)
EndFunction

Function AnimateLocalMissile(ObjectReference akMissile, Bool abSyncImpact = True)
    Int polls = 0
    While !akMissile.Is3DLoaded() && polls < 50 && !bNukeTriggered && IsPlayerInBlastWorld(akMissile)
        Utility.Wait(0.1)
        polls += 1
    EndWhile
    If akMissile.Is3DLoaded() && !bNukeTriggered && IsPlayerInBlastWorld(akMissile)
        Bool played = akMissile.PlayGamebryoAnimation("PlayAnim01", True)
        Int soundInstance = -1
        If FXProjectileMissileICBMWarheadReentry != None
            soundInstance = FXProjectileMissileICBMWarheadReentry.Play(akMissile)
        EndIf
        ; PlayAnim01's path reaches ground at its 6.666666 key, after the old 6.1318 cutoff.
        ; Late arrival keeps the impact timer that was armed while the player was away.
        If abSyncImpact
            StartTimer(6.666667, 8)
        EndIf
        Debug.Trace("[B21 Nuke] Missile playback=" + played + " loaded=" + akMissile.Is3DLoaded() + " sound=" + soundInstance + " polls=" + polls + " position=" + akMissile.GetPositionX() + "," + akMissile.GetPositionY() + "," + akMissile.GetPositionZ() + " scale=" + akMissile.GetScale() + " syncImpact=" + abSyncImpact)
    Else
        Debug.Trace("[B21 Nuke] Missile model unavailable before impact: " + akMissile)
    EndIf
EndFunction

Function DetonateLocalBlast()
    If bNukeTriggered
        Return
    EndIf
    ObjectReference blastMarker = NukeBlastMarker.GetReference()
    If blastMarker == None
        Debug.Trace("[B21 Nuke] Detonation failed: blast alias is empty.")
        Return
    EndIf
    bNukeTriggered = True
    CancelTimer(2)
    CancelTimer(8)
    Debug.Trace("[B21 Nuke] Impact started: marker=" + blastMarker)
    ObjectReference missile = IncomingNuke.GetReference()
    If missile != None
        missile.DisableNoWait()
    EndIf
    ObjectReference warningMarker = AudioWarningMarker.GetReference()
    If warningMarker != None
        warningMarker.DisableNoWait()
    EndIf
    Bool localImpact = IsPlayerInBlastWorld(blastMarker)
    If localImpact && Nuke76Explosion != None
        blastMarker.PlaceAtMe(Nuke76Explosion)
    EndIf
    bNukeTouchdown = True
    Debug.Trace("[B21 Nuke] Detonation at " + blastMarker.GetPositionX() + "," + blastMarker.GetPositionY() + "," + blastMarker.GetPositionZ() + " explosion=" + Nuke76Explosion)
    ObjectReference blastArtRef = NukeBlastArt.GetReference()
    If localImpact && blastArtRef != None
        blastArtRef.MoveTo(blastMarker)
        Var[] skyArgs = new Var[2]
        skyArgs[0] = blastArtRef
        skyArgs[1] = blastMarker.GetWorldSpace()
        Utility.CallGlobalFunction("B21_NukeZones", "PrepareCloud", skyArgs)
        blastArtRef.EnableNoWait()
        Var[] artArgs = new Var[1]
        artArgs[0] = blastArtRef
        CallFunctionNoWait("AnimateLocalBlastArt", artArgs)
    EndIf
    ObjectReference cloudRef = DistantCloud.GetReference()
    If cloudRef != None
        cloudRef.DisableNoWait()
    EndIf

    Actor player = LaunchingPlayer.GetReference() as Actor
    If player == None
        player = Game.GetPlayer()
    EndIf
    Float playerDistance = 9999999.0
    If localImpact
        playerDistance = blastMarker.GetDistance(player)
    EndIf
    Debug.Trace("[B21 Nuke] Blast distance to player=" + playerDistance)
    If MUS76SpecialNuke != None && playerDistance <= EN07_Blast_MusicRadius.GetValue()
        MUS76SpecialNuke.Add()
        StartTimer(60.0, 11)
    EndIf
    Float blastDistance = EN07_NukeBlastDistance.GetValue()
    If blastDistance <= 0.0
        blastDistance = 20460.0
    EndIf
    If playerDistance <= blastDistance
        If BlastSpell != None
            BlastSpell.Cast(player, player)
        ElseIf EN07_ApplyBlastVisualEffectSpell != None
            EN07_ApplyBlastVisualEffectSpell.Cast(player, player)
        EndIf
        Float vaporizeDistance = EN07_Blast_ExplosionDistanceGlobal.GetValue()
        If vaporizeDistance > 0.0 && playerDistance <= vaporizeDistance
            EN07_ApplyVaporizeVisualEffectSpell.Cast(player, player)
            EN07_Blast_KilledByPlayerNuke.Show()
        EndIf
    ElseIf playerDistance <= EN07_Blast_MusicRadius.GetValue()
        EN07_ApplyBlastDistantVisualEffectSpell.Cast(player, player)
    EndIf

    player.SetValue(MQ_Overseer_NukesLaunchedValue, player.GetValue(MQ_Overseer_NukesLaunchedValue) + 1.0)
    Quest fleeQuest = Self as Quest
    fleeQuest.SetObjectiveCompleted(iFleeObjID, True)
    EN07_NukeMasterScript masterScript = EN07_MQ_Nuke_Master as EN07_NukeMasterScript
    If masterScript != None
        masterScript.CompleteLocalLaunch(iDebugValue, iDebugwithCountdownValue)
    EndIf
    If !fleeQuest.IsStageDone(100)
        fleeQuest.SetStage(100)
    EndIf
EndFunction

Function ClearLocalBlastArt()
    ObjectReference blastArt = NukeBlastArt.GetReference()
    If blastArt != None
        blastArt.DisableNoWait()
        Debug.Trace("[B21 Nuke] Mushroom cloud removed: " + blastArt)
    EndIf
EndFunction

Function AnimateLocalBlastArt(ObjectReference akBlastArt)
    Int polls = 0
    While !akBlastArt.Is3DLoaded() && polls < 50 && IsPlayerInBlastWorld(akBlastArt)
        Utility.Wait(0.1)
        polls += 1
    EndWhile
    If akBlastArt.Is3DLoaded() && IsPlayerInBlastWorld(akBlastArt)
        Bool played = akBlastArt.PlayGamebryoAnimation("PlayAnim01", True)
        Debug.Trace("[B21 Nuke] Mushroom cloud animation=" + played + " reference=" + akBlastArt)
    Else
        Debug.Trace("[B21 Nuke] Mushroom cloud model unavailable at target: " + akBlastArt)
    EndIf
    StartTimer(95.1, 9)
EndFunction

Event OnTimer(Int aiTimerID)
    Debug.Trace("[B21 Nuke] Blast timer fired: " + aiTimerID)
    If aiTimerID == 11
        If MUS76SpecialNuke != None
            MUS76SpecialNuke.Remove()
        EndIf
    ElseIf aiTimerID == 2
        BeginLocalDescent()
    ElseIf aiTimerID == 8
        DetonateLocalBlast()
    ElseIf aiTimerID == 9 && bNukeTouchdown
        ClearLocalBlastArt()
    EndIf
EndEvent

