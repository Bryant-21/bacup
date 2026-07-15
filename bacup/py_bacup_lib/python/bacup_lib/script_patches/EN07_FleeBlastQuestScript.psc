Bool Function BeginLocalBlast(ObjectReference akBlastMarker, Actor akLaunchingPlayer, Int aiSiloID, Int aiLaunchID, Spell akSmokeSpell, Spell akBlastSpell)
    If akBlastMarker == None
        Return False
    EndIf
    SmokeSpell = akSmokeSpell
    BlastSpell = akBlastSpell
    bNukeTriggered = False
    bNukeTouchdown = False
    iDebugValue = aiSiloID
    iDebugwithCountdownValue = aiLaunchID
    NukeBlastMarker.ForceRefTo(akBlastMarker)
    If akLaunchingPlayer == None
        akLaunchingPlayer = Game.GetPlayer()
    EndIf
    LaunchingPlayer.ForceRefTo(akLaunchingPlayer)
    Location blastLocation = akBlastMarker.GetCurrentLocation()
    If blastLocation != None
        TriggerLocation.ForceLocationTo(blastLocation)
    EndIf

    Quest fleeQuest = Self as Quest
    Bool questWasRunning = fleeQuest.IsRunning()
    If !questWasRunning
        fleeQuest.Start()
    EndIf
    NukeBlastMarker.ForceRefTo(akBlastMarker)
    LaunchingPlayer.ForceRefTo(akLaunchingPlayer)
    If !fleeQuest.IsStageDone(10)
        fleeQuest.SetStage(10)
    ElseIf questWasRunning
        HandleStage(10)
    EndIf
    Return fleeQuest.IsRunning()
EndFunction

Function HandleStage(Int aiStage)
    If aiStage == 10
        BeginLocalCountdown()
    ElseIf aiStage == 100
        DetonateLocalBlast()
    EndIf
EndFunction

Function BeginLocalCountdown()
    ObjectReference blastMarker = NukeBlastMarker.GetReference()
    Actor player = LaunchingPlayer.GetReference() as Actor
    If player == None
        player = Game.GetPlayer()
    EndIf
    If blastMarker == None
        Return
    EndIf

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
    FXProjectileMissileICBMWarheadReentry.Play(blastMarker)
    If EN07_ApplySoundCategorySpell != None
        EN07_ApplySoundCategorySpell.Cast(player, player)
    EndIf
    If SmokeSpell != None
        SmokeSpell.Cast(player, player)
    EndIf
    EN07_Blast_NukeLaunchedByPlayer.Show()

    Quest fleeQuest = Self as Quest
    fleeQuest.SetObjectiveDisplayed(iFleeObjID, True, True)
    Float countdown = fNukeDropTimerLength
    If countdown <= 0.0
        countdown = 180.0
    EndIf
    StartTimer(countdown, iNukeBlastTimerID)
EndFunction

Function DetonateLocalBlast()
    If bNukeTriggered
        Return
    EndIf
    bNukeTriggered = True
    ObjectReference blastMarker = NukeBlastMarker.GetReference()
    If blastMarker == None
        Return
    EndIf

    ObjectReference blastArtRef = NukeBlastArt.GetReference()
    If blastArtRef != None
        blastArtRef.MoveTo(blastMarker)
        blastArtRef.Enable()
        EN07_ExplosionMeshRefScript blastArtScript = blastArtRef as EN07_ExplosionMeshRefScript
        If blastArtScript != None
            blastArtScript.ClientExplosionReset()
            blastArtScript.ClientExplosionAmin()
        EndIf
    EndIf
    ObjectReference cloudRef = DistantCloud.GetReference()
    If cloudRef != None
        cloudRef.MoveTo(blastMarker)
        cloudRef.Enable()
    EndIf
    If Nuke76Explosion != None
        blastMarker.PlaceAtMe(Nuke76Explosion)
    EndIf

    Actor player = LaunchingPlayer.GetReference() as Actor
    If player == None
        player = Game.GetPlayer()
    EndIf
    Float playerDistance = player.GetDistance(blastMarker)
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
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == iNukeBlastTimerID
        Quest fleeQuest = Self as Quest
        If !fleeQuest.IsStageDone(100)
            fleeQuest.SetStage(100)
        Else
            DetonateLocalBlast()
        EndIf
    EndIf
EndEvent
