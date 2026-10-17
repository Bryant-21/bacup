Event OnInit()
    ; A pack-in creates its station before it wires the station to the pod,
    ; and the actor must link to the pod, so spawn after the cell settles.
    StartTimer(2.0, 3)
EndEvent

Event OnLoad()
    ; Spawning goes through timer 3 only: restarting it replaces a pending OnInit
    ; spawn, so the two paths cannot both pass the guard across PlaceActorAtMe.
    If createdActorRef == None && Self.GetLinkedRef(WorkshopLinkCreatedActorTarget) != None
        StartTimer(0.5, 3)
    EndIf
EndEvent

Event OnWorkshopObjectDestroyed(ObjectReference akActionRef)
    ; Scrapping the station must not leave its essential robot behind.
    CancelTimer(3)
    If createdActorRef != None
        UnregisterForRemoteEvent(createdActorRef, "OnDeath")
        createdActorRef.DisableNoWait()
        createdActorRef.Delete()
        createdActorRef = None
    EndIf
EndEvent

Function SpawnCreatedActor()
    If createdActorRef != None && !createdActorRef.IsDead()
        Return
    EndIf
    If CreatedActorBase == None
        Return
    EndIf
    workshopRef = Self.GetLinkedRef(WorkshopItemKeyword)
    ; Collectron robots sit in a separate pod furniture that the station links
    ; to; their wander/sit package reads the pod through CreatedActorBaseLink.
    ObjectReference linkTarget = Self.GetLinkedRef(WorkshopLinkCreatedActorTarget)
    If linkTarget == None
        linkTarget = Self
    EndIf
    createdActorRef = linkTarget.PlaceActorAtMe(CreatedActorBase)
    If createdActorRef == None
        Return
    EndIf
    If WorkshopItemKeyword != None
        createdActorRef.AddKeyword(WorkshopItemKeyword)
    EndIf
    If CreatedActorBaseRefType != None
        createdActorRef.SetLinkedRef(Self, CreatedActorBaseRefType)
    ElseIf CreatedActorBaseLink != None
        createdActorRef.SetLinkedRef(linkTarget, CreatedActorBaseLink)
    ElseIf linkTarget != Self
        ; The Biv Bev station (79A1C9) binds no CreatedActorBaseLink; every other
        ; Collectron binds LinkProtectronPod, which WorkshopScavengePackage sits at.
        createdActorRef.SetLinkedRef(linkTarget, Game.GetFormFromFile(0x0F39E3, "Fallout4.esm") as Keyword)
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
    If aiTimerID == 3
        SpawnCreatedActor()
    ElseIf aiTimerID == DestroyAfterDeathTimerID
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
