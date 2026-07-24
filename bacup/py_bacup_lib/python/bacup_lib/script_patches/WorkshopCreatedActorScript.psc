Event OnInit()
    SpawnCreatedActor()
EndEvent

Function SpawnCreatedActor()
    If createdActorRef != None && !createdActorRef.IsDead()
        Return
    EndIf
    If CreatedActorBase == None
        Return
    EndIf
    workshopRef = Self.GetLinkedRef()
    createdActorRef = Self.PlaceActorAtMe(CreatedActorBase)
    If createdActorRef == None
        Return
    EndIf
    If WorkshopItemKeyword != None
        createdActorRef.AddKeyword(WorkshopItemKeyword)
    EndIf
    If CreatedActorBaseRefType != None
        createdActorRef.SetLinkedRef(Self, CreatedActorBaseRefType)
    ElseIf CreatedActorBaseLink != None
        createdActorRef.SetLinkedRef(Self, CreatedActorBaseLink)
    EndIf
    If workshopRef != None && WorkshopLinkCreatedActorTarget != None
        createdActorRef.SetLinkedRef(workshopRef, WorkshopLinkCreatedActorTarget)
    EndIf
    DestroyAfterDeathSeconds = DestroyAfterDeathSecondsMin
    If StartsDestroyed
        createdActorRef.Disable()
    EndIf
    RegisterForRemoteEvent(createdActorRef, "OnDeath")
EndFunction

Event Actor.OnDeath(Actor akSender, Actor akKiller)
    If akSender != createdActorRef
        Return
    EndIf
    UnregisterForRemoteEvent(createdActorRef, "OnDeath")
    StartTimer(DestroyAfterDeathSeconds, DestroyAfterDeathTimerID)
    If DestroyAfterDeathResetSeconds > 0.0
        StartTimer(DestroyAfterDeathResetSeconds, DestroyAfterDeathResetTimerID)
    EndIf
EndEvent

Event OnTimer(int aiTimerID)
    If aiTimerID == DestroyAfterDeathTimerID
        RemoveCreatedActor()
    ElseIf aiTimerID == DestroyAfterDeathResetTimerID
        DestroyAfterDeathSeconds = DestroyAfterDeathSecondsMin
    EndIf
EndEvent

Function RemoveCreatedActor()
    If createdActorRef == None
        Return
    EndIf
    If ActorAliasID >= 0 && QuestToRemoveFrom != None
        ReferenceAlias targetAlias = QuestToRemoveFrom.GetAlias(ActorAliasID) as ReferenceAlias
        If targetAlias != None
            targetAlias.Clear()
        EndIf
    EndIf
    If DeleteActorWhenDestroyed
        createdActorRef.Delete()
        createdActorRef = None
        DestroyAfterDeathSeconds = DestroyAfterDeathSecondsMin + DestroyAfterDeathSecondsAddPerDeath
        If DestroyAfterDeathSecondsMax > 0.0 && DestroyAfterDeathSeconds > DestroyAfterDeathSecondsMax
            DestroyAfterDeathSeconds = DestroyAfterDeathSecondsMax
        EndIf
        SpawnCreatedActor()
    Else
        createdActorRef = None
    EndIf
EndFunction
