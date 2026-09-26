Event OnAliasInit()
    RegisterOwnerCombat()
EndEvent

Event OnLoad()
    RegisterOwnerCombat()
EndEvent

Function RegisterOwnerCombat()
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsRunning() || InstanceOwner == None
        Return
    EndIf
    Actor ownerRef = InstanceOwner.GetActorReference()
    If ownerRef == None || ownerRef != Game.GetPlayer()
        Return
    EndIf
    RegisterForRemoteEvent(ownerRef, "OnCombatStateChanged")
    RegisterForRemoteEvent(ownerRef, "OnPlayerLoadGame")
    ClearGhostForOwnerCombat()
EndFunction

Function ClearGhostForOwnerCombat()
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsRunning() || InstanceOwner == None
        Return
    EndIf
    Actor ownerRef = InstanceOwner.GetActorReference()
    Actor aliasActor = GetActorReference()
    If ownerRef != None && ownerRef == Game.GetPlayer() && aliasActor != None
        If (ownerRef.IsInCombat() && ownerRef.GetCombatTarget() == aliasActor) || (aliasActor.IsInCombat() && aliasActor.GetCombatTarget() == ownerRef)
            aliasActor.SetGhost(False)
        EndIf
    EndIf
EndFunction

Event Actor.OnCombatStateChanged(Actor akSender, Actor akTarget, Int aeCombatState)
    If InstanceOwner != None && akSender == InstanceOwner.GetActorReference() && akTarget == GetActorReference() && aeCombatState == 1
        ClearGhostForOwnerCombat()
    EndIf
EndEvent

Event OnCombatStateChanged(Actor akTarget, Int aeCombatState)
    If InstanceOwner != None && akTarget == InstanceOwner.GetActorReference() && aeCombatState == 1
        ClearGhostForOwnerCombat()
    EndIf
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        RegisterOwnerCombat()
    EndIf
EndEvent

Event OnAliasShutdown()
    UnregisterForAllRemoteEvents()
EndEvent
