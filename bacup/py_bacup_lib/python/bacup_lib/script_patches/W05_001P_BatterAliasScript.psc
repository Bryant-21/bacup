Event OnAliasInit()
    Quest owningQuest = GetOwningQuest()
    If owningQuest
        RegisterForRemoteEvent(owningQuest, "OnStageSet")
        If owningQuest.IsStageDone(RegistrationStage)
            StartHitRegistration()
        EndIf
    EndIf
EndEvent

Event Quest.OnStageSet(Quest akSender, int auiStageID, int auiItemID)
    If akSender == GetOwningQuest() && auiStageID == RegistrationStage
        StartHitRegistration()
    EndIf
EndEvent

Function StartHitRegistration()
    PlayerHitBatter = False
    MortHitBatter = False
    UnregisterForAllHitEvents()
    RegisterForHitEvent(Self)
EndFunction

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, bool abPowerAttack, bool abSneakAttack, bool abBashAttack, bool abHitBlocked, string apMaterial)
    Actor playerRef = OwningPlayer.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf

    If akAggressor == playerRef
        PlayerHitBatter = True
    ElseIf Mort && akAggressor == Mort.GetActorReference()
        MortHitBatter = True
    EndIf

    Actor batterRef = akTarget as Actor
    If batterRef && !batterRef.IsDead()
        RegisterForHitEvent(Self)
    EndIf
EndEvent

Event OnAliasReset()
    PlayerHitBatter = False
    MortHitBatter = False
    UnregisterForAllHitEvents()
EndEvent

Event OnDeath(Actor akKiller)
    Quest owningQuest = GetOwningQuest()
    Actor playerRef = OwningPlayer.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf

    If playerRef != None
        Actor mortRef = Mort.GetActorReference()
        If akKiller == playerRef
            If playerRef.GetValue(W05_MQ_001P_Wayward_PlayerKilledBatter) < 1.0
                playerRef.SetValue(W05_MQ_001P_Wayward_PlayerKilledBatter, 1.0)
            EndIf
        ElseIf mortRef != None && akKiller == mortRef
            If playerRef.GetValue(W05_MQ_001P_Wayward_MortKilledBatter) < 1.0
                playerRef.SetValue(W05_MQ_001P_Wayward_MortKilledBatter, 1.0)
            EndIf
        EndIf
    EndIf

    If owningQuest != None
        If !owningQuest.IsStageDone(470)
            owningQuest.SetStage(470)
        EndIf
        If !owningQuest.IsStageDone(599)
            owningQuest.SetStage(599)
        EndIf
    EndIf
EndEvent
