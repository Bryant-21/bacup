ObjectReference Function AttackTargetRef()
    If Alias_AttackTarget != None && Alias_AttackTarget.GetReference() != None
        Return Alias_AttackTarget.GetReference()
    EndIf
    If Alias_Workshop != None
        Return Alias_Workshop.GetReference()
    EndIf
    Return None
EndFunction

Int Function DesiredScorchbeastCount()
    Int minimum = ScorchbeastCountMin
    If minimum < 1
        minimum = 1
    EndIf
    Int maximum = ScorchbeastCountMax
    If maximum < minimum
        maximum = minimum
    EndIf
    ; One player cannot fight an entire FO76 flight; the EWS live cap is the same idea.
    If maximum > 3
        maximum = 3
    EndIf
    Return Utility.RandomInt(minimum, maximum)
EndFunction

Int Function CountLivingScorchbeasts()
    Int living = 0
    If Alias_ScorchbeastCollection != None
        Int index = 0
        While index < Alias_ScorchbeastCollection.GetCount()
            Actor member = Alias_ScorchbeastCollection.GetAt(index) as Actor
            If member != None && !member.IsDead()
                living += 1
            EndIf
            index += 1
        EndWhile
    EndIf
    Int slot = 0
    While Alias_Scorchbeasts != None && slot < Alias_Scorchbeasts.Length
        ReferenceAlias scorchbeastAlias = Alias_Scorchbeasts[slot]
        Actor held = None
        If scorchbeastAlias != None
            held = scorchbeastAlias.GetActorReference()
        EndIf
        If held != None && !held.IsDead() && (Alias_ScorchbeastCollection == None || Alias_ScorchbeastCollection.Find(held) < 0)
            living += 1
        EndIf
        slot += 1
    EndWhile
    Return living
EndFunction

Function ClaimScorchbeast(Actor akScorchbeast)
    If akScorchbeast == None
        Return
    EndIf
    If B21SpawnedScorchbeasts == None
        B21SpawnedScorchbeasts = New Actor[0]
    EndIf
    If B21SpawnedScorchbeasts.Find(akScorchbeast) < 0
        B21SpawnedScorchbeasts.Add(akScorchbeast)
    EndIf
    If Alias_ScorchbeastCollection != None && Alias_ScorchbeastCollection.Find(akScorchbeast) < 0
        Alias_ScorchbeastCollection.AddRef(akScorchbeast)
    EndIf
    Int slot = 0
    While Alias_Scorchbeasts != None && slot < Alias_Scorchbeasts.Length
        ReferenceAlias scorchbeastAlias = Alias_Scorchbeasts[slot]
        If scorchbeastAlias != None && scorchbeastAlias.GetReference() == None
            scorchbeastAlias.ForceRefTo(akScorchbeast)
            Return
        EndIf
        slot += 1
    EndWhile
EndFunction

Bool Function SummonScorchbeasts()
    If IsStopping() || IsStopped() || IsCompleted()
        Return False
    EndIf
    ObjectReference targetRef = AttackTargetRef()
    If targetRef == None
        Return False
    EndIf
    If CountLivingScorchbeasts() > 0
        WatchScorchbeastProximity()
        Return True
    EndIf

    ; FO76's server handed out roaming scorchbeasts; the converted plugin's verified
    ; LvlScorchBeast stands in for that service.
    ActorBase scorchbeastBase = Game.GetFormFromFile(0x000974BC, "SeventySix.esm") as ActorBase
    If scorchbeastBase == None
        If NoScorchbeastStage >= 0 && !IsStageDone(NoScorchbeastStage)
            SetStage(NoScorchbeastStage)
        EndIf
        Return False
    EndIf

    Int wanted = DesiredScorchbeastCount()
    Int spawned = 0
    While spawned < wanted
        Actor scorchbeast = targetRef.PlaceAtMe(scorchbeastBase, 1, False, False, True) as Actor
        If scorchbeast != None
            ClaimScorchbeast(scorchbeast)
            If SQ_ScorchbeastKeyword != None
                scorchbeast.AddKeyword(SQ_ScorchbeastKeyword)
            EndIf
            Actor targetActor = targetRef as Actor
            If targetActor != None && !targetActor.IsDead()
                scorchbeast.StartCombat(targetActor, True)
            Else
                Actor playerRef = Game.GetPlayer()
                If playerRef != None
                    scorchbeast.StartCombat(playerRef)
                EndIf
            EndIf
        EndIf
        spawned += 1
    EndWhile

    If CountLivingScorchbeasts() <= 0
        If NoScorchbeastStage >= 0 && !IsStageDone(NoScorchbeastStage)
            SetStage(NoScorchbeastStage)
        EndIf
        Return False
    EndIf
    WatchScorchbeastProximity()
    Return True
EndFunction

Function WatchScorchbeastProximity()
    If ScorchbeastNearTargetStage < 0 || IsStageDone(ScorchbeastNearTargetStage)
        Return
    EndIf
    CancelTimer(34612)
    StartTimer(2.0, 34612)
EndFunction

Function DismissScorchbeasts()
    CancelTimer(34611)
    CancelTimer(34612)
    Int index = 0
    While B21SpawnedScorchbeasts != None && index < B21SpawnedScorchbeasts.Length
        Actor scorchbeast = B21SpawnedScorchbeasts[index]
        If scorchbeast != None
            If Alias_ScorchbeastCollection != None
                Alias_ScorchbeastCollection.RemoveRef(scorchbeast)
            EndIf
            If !scorchbeast.IsDead()
                scorchbeast.DisableNoWait()
            EndIf
            scorchbeast.Delete()
        EndIf
        index += 1
    EndWhile
    B21SpawnedScorchbeasts = None
EndFunction

Event OnQuestInit()
    B21SpawnedScorchbeasts = None
    B21ScorchbeastSummonTries = 0
    ; Every binder's own script is hollow, so the attack starts itself: FO76 raised the
    ; scorchbeast as soon as the quest instance came up.
    StartTimer(3.0, 34611)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 34611
        B21ScorchbeastSummonTries += 1
        If !SummonScorchbeasts() && AttackTargetRef() == None && B21ScorchbeastSummonTries < 10 && IsRunning()
            ; The attack target can be an alias that fills after the quest starts.
            StartTimer(3.0, 34611)
        EndIf
    ElseIf aiTimerID == 34612
        If ScorchbeastNearTargetStage < 0 || IsStageDone(ScorchbeastNearTargetStage) || !IsRunning()
            Return
        EndIf
        ObjectReference targetRef = AttackTargetRef()
        Int index = 0
        While targetRef != None && B21SpawnedScorchbeasts != None && index < B21SpawnedScorchbeasts.Length
            Actor scorchbeast = B21SpawnedScorchbeasts[index]
            If scorchbeast != None && !scorchbeast.IsDead() && scorchbeast.GetDistance(targetRef) <= 3000.0
                SetStage(ScorchbeastNearTargetStage)
                Return
            EndIf
            index += 1
        EndWhile
        StartTimer(2.0, 34612)
    EndIf
EndEvent

Event OnQuestShutdown()
    DismissScorchbeasts()
EndEvent
