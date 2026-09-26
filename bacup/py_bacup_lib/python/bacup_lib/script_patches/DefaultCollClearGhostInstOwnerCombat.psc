Event OnCombatStateChanged(ObjectReference akSenderRef, Actor akTarget, int aeCombatState)
    If aeCombatState == 1 && InstanceOwner != None && akTarget == InstanceOwner.GetActorReference() && Find(akSenderRef) >= 0
        ClearCrewGhosting()
    EndIf
EndEvent

Event OnAliasInit()
    DoOnce = False
    CheckCrewCombat()
EndEvent

Event OnLoad(ObjectReference akSenderRef)
    CheckCrewCombat()
EndEvent

Function CheckCrewCombat()
    If InstanceOwner == None
        Return
    EndIf
    Actor ownerRef = InstanceOwner.GetActorReference()
    If ownerRef == None || ownerRef != Game.GetPlayer()
        Return
    EndIf
    Int i = 0
    While i < GetCount()
        Actor crewActor = GetAt(i) as Actor
        If crewActor != None && crewActor.IsInCombat() && crewActor.GetCombatTarget() == ownerRef
            ClearCrewGhosting()
            Return
        EndIf
        i += 1
    EndWhile
EndFunction

Function ClearCrewGhosting()
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsRunning() || InstanceOwner == None || InstanceOwner.GetActorReference() != Game.GetPlayer()
        Return
    EndIf
    Int i = 0
    While i < GetCount()
        Actor crewActor = GetAt(i) as Actor
        If crewActor != None
            crewActor.SetGhost(False)
        EndIf
        i += 1
    EndWhile
    DoOnce = True
EndFunction
