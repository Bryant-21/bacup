Bool Function IsEventResolved()
    If IsStopping() || IsStopped() || IsCompleted()
        Return True
    EndIf
    Return IsStageDone(500) || IsStageDone(8900)
EndFunction

Function PublishMarkVariables()
    Quest owner = Self as Quest
    B21:QuestVariables variables = owner as B21:QuestVariables
    If variables == None
        Return
    EndIf
    variables.SetVariable("EnemiesMarked", MarkedEnemyCount() as Float)
    variables.SetVariable("EnemiesToMark", RequiredMarkCount() as Float)
EndFunction

Int Function LivingScorchedCount()
    Int living = 0
    Int index = 0
    While ScorchedEnemies != None && index < ScorchedEnemies.GetCount()
        Actor enemy = ScorchedEnemies.GetAt(index) as Actor
        If enemy != None && !enemy.IsDead()
            living += 1
        EndIf
        index += 1
    EndWhile
    Return living
EndFunction

Int Function MarkedEnemyCount()
    If TaggedEnemies == None
        Return 0
    EndIf
    Return TaggedEnemies.GetCount()
EndFunction

Int Function RequiredMarkCount()
    Quest owner = Self as Quest
    B21:QuestVariables variables = owner as B21:QuestVariables
    Int required = 0
    If variables != None
        required = variables.GetVariable("EnemiesToMark") as Int
    EndIf
    If required > 0
        Return required
    EndIf

    Int minimum = MinScorchedCount
    If minimum < 1
        minimum = 1
    EndIf
    Int maximum = MaxScorchedCount
    If maximum < minimum
        maximum = minimum
    EndIf
    required = Utility.RandomInt(minimum, maximum)
    Int available = ScorchedEnemies.GetCount()
    If available > 0 && required > available
        required = available
    EndIf
    If variables != None
        variables.SetVariable("EnemiesToMark", required as Float)
    EndIf
    Return required
EndFunction

Bool Function MarkEnemyInPlayerSights()
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || TaggedEnemies == None || ScorchedEnemies == None
        Return False
    EndIf
    ; FO76 tagged targets through a recon scope, a service FO4 has no API for: aiming
    ; down the sights at a Scorched in line of sight stands in for the scope tag.
    If !playerRef.IsInIronSights() && !playerRef.IsWeaponDrawn()
        Return False
    EndIf

    Int index = 0
    While index < ScorchedEnemies.GetCount()
        Actor enemy = ScorchedEnemies.GetAt(index) as Actor
        If enemy != None && !enemy.IsDead() && TaggedEnemies.Find(enemy) < 0
            If playerRef.GetDistance(enemy) <= 4096.0 && Math.Abs(playerRef.GetHeadingAngle(enemy)) <= 20.0 && playerRef.HasDetectionLOS(enemy)
                TaggedEnemies.AddRef(enemy)
                Return True
            EndIf
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

Function StartMarkingPhase()
    If IsEventResolved() || IsStageDone(200)
        Return
    EndIf
    PublishMarkVariables()
    CancelTimer(31371)
    StartTimer(1.0, 31371)
EndFunction

Function StopMarkingPhase()
    CancelTimer(31371)
EndFunction

Function BroadcastEventTopic()
    If pBoSZ03EventEBSTopic == None
        Return
    EndIf
    SQ_MasterScript masterScript = SQ_Master as SQ_MasterScript
    If masterScript == None
        Return
    EndIf
    Actor speaker = masterScript.EBSDefaultActor
    If speaker != None
        speaker.Say(pBoSZ03EventEBSTopic)
    EndIf
EndFunction

Function BeginArtilleryBarrage()
    If IsEventResolved() || IsStageDone(350)
        Return
    EndIf
    BlastTargets = New ObjectReference[0]
    Int index = 0
    While TaggedEnemies != None && index < TaggedEnemies.GetCount() && BlastTargets.Length < 6
        Actor marked = TaggedEnemies.GetAt(index) as Actor
        If marked != None && !marked.IsDead()
            BlastTargets.Add(marked)
        EndIf
        index += 1
    EndWhile
    index = 0
    While ScorchedEnemies != None && index < ScorchedEnemies.GetCount() && BlastTargets.Length < 6
        Actor enemy = ScorchedEnemies.GetAt(index) as Actor
        If enemy != None && !enemy.IsDead() && BlastTargets.Find(enemy) < 0
            BlastTargets.Add(enemy)
        EndIf
        index += 1
    EndWhile
    If BlastTargets.Length == 0 && BlastMarker != None && BlastMarker.GetReference() != None
        BlastTargets.Add(BlastMarker.GetReference())
    EndIf

    CurrentBlastTarget = None
    StrikesOnCurrentTarget = 0
    CancelTimer(ArtilleryStrikeTimerID)
    CancelTimer(ArtilleryWaitForFinishedTimerID)
    StartTimer(1.0, ArtilleryStrikeTimerID)
    StartTimer(60.0, ArtilleryWaitForFinishedTimerID)
EndFunction

Function FireNextArtilleryStrike()
    If IsEventResolved() || IsStageDone(350) || !IsRunning()
        Return
    EndIf
    If CurrentBlastTarget == None || StrikesOnCurrentTarget >= 3
        If BlastTargets == None || BlastTargets.Length == 0
            SetStage(350)
            Return
        EndIf
        CurrentBlastTarget = BlastTargets[0]
        BlastTargets.Remove(0)
        StrikesOnCurrentTarget = 0
    EndIf

    ObjectReference markerRef = None
    If BlastMarker != None
        markerRef = BlastMarker.GetReference()
    EndIf
    If markerRef != None && CurrentBlastTarget != None
        markerRef.MoveTo(CurrentBlastTarget)
        ; FO4's own artillery drops this shooter activator at the impact point.
        If WorkshopArtilleryStrikeProjectileShooterFar != None
            markerRef.PlaceAtMe(WorkshopArtilleryStrikeProjectileShooterFar, 1, False, False, True)
        EndIf
        If FXProjectileArtilleryMinutemen != None
            FXProjectileArtilleryMinutemen.Play(markerRef)
        EndIf
    EndIf

    StrikesOnCurrentTarget += 1
    Float delay = Utility.RandomFloat(firingArtilleryMinSeconds, firingArtilleryMaxSeconds)
    If delay < 0.5
        delay = 0.5
    EndIf
    StartTimer(delay, ArtilleryStrikeTimerID)
EndFunction

Function StopArtilleryBarrage()
    CancelTimer(ArtilleryStrikeTimerID)
    CancelTimer(ArtilleryWaitForFinishedTimerID)
    BlastTargets = None
    CurrentBlastTarget = None
    StrikesOnCurrentTarget = 0
EndFunction

Event OnQuestInit()
    Quest owner = Self as Quest
    B21:QuestTimer questTimer = owner as B21:QuestTimer
    If questTimer != None
        RegisterForCustomEvent(questTimer, "QuestTimerEnded")
    EndIf
EndEvent

Event B21:QuestTimer.QuestTimerEnded(B21:QuestTimer akSender, Var[] akArgs)
    ; The activity's 30-minute window is the only failure the source data defines.
    If IsEventResolved()
        Return
    EndIf
    SetStage(8900)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 31371
        If IsEventResolved() || IsStageDone(200)
            Return
        EndIf
        If MarkEnemyInPlayerSights()
            PublishMarkVariables()
            If MarkedEnemyCount() >= RequiredMarkCount()
                SetStage(200)
                Return
            EndIf
        EndIf
        StartTimer(1.0, 31371)
    ElseIf aiTimerID == ArtilleryStrikeTimerID
        FireNextArtilleryStrike()
    ElseIf aiTimerID == ArtilleryWaitForFinishedTimerID
        If !IsEventResolved() && !IsStageDone(350)
            SetStage(350)
        EndIf
    EndIf
EndEvent

Event OnQuestShutdown()
    StopMarkingPhase()
    StopArtilleryBarrage()
    Quest owner = Self as Quest
    B21:QuestTimer questTimer = owner as B21:QuestTimer
    If questTimer != None
        UnregisterForCustomEvent(questTimer, "QuestTimerEnded")
    EndIf
EndEvent
