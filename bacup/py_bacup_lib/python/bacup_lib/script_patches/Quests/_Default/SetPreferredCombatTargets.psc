Event OnAliasInit()
    If !UseOnRefAddedTiming
        ReconcilePreferredCombatTargets()
    EndIf
EndEvent

Event OnLoad(ObjectReference akSenderRef)
    ApplyPreferredCombatTarget(akSenderRef)
EndEvent

Event OnWorkshopObjectRepaired(ObjectReference akSenderRef, ObjectReference akReference)
    If SetTargetOnRepaired
        ApplyPreferredCombatTarget(akSenderRef)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 7701
        DamagePreferredObjectTargets()
    EndIf
EndEvent

Function ReconcilePreferredCombatTargets()
    Int index = 0
    While index < GetCount()
        ApplyPreferredCombatTarget(GetAt(index))
        index += 1
    EndWhile
EndFunction

Bool Function ApplyPreferredCombatTarget(ObjectReference sourceRef)
    Actor sourceActor = sourceRef as Actor
    If sourceActor == None || sourceActor.IsDead()
        Return False
    EndIf

    Actor targetActor = FindPreferredCombatTarget(sourceActor)
    If targetActor == None
        ; FO4 AI cannot pick a non-actor combat target, so nearby members wear intact preferred objects down on a timer instead.
        If HasIntactPreferredObject()
            StartTimer(2.0, 7701)
        EndIf
        Return False
    EndIf

    If PersistTargets && PersistCombatTargetsAfterCombatEndKeyword != None && !sourceActor.HasKeyword(PersistCombatTargetsAfterCombatEndKeyword)
        sourceActor.AddKeyword(PersistCombatTargetsAfterCombatEndKeyword)
    EndIf
    If StartCombat
        sourceActor.StartCombat(targetActor, True)
    EndIf
    Return StartCombat
EndFunction

Actor Function FindPreferredCombatTarget(Actor sourceActor)
    Int index = 0
    While PreferredTargets != None && index < PreferredTargets.Length
        ReferenceAlias targetAlias = PreferredTargets[index]
        If targetAlias != None
            Actor targetActor = targetAlias.GetActorReference()
            If targetActor != None && targetActor != sourceActor && !targetActor.IsDead()
                Return targetActor
            EndIf
        EndIf
        index += 1
    EndWhile

    index = 0
    While PreferredTargetCollection != None && index < PreferredTargetCollection.GetCount()
        Actor targetActor = PreferredTargetCollection.GetActorAt(index)
        If targetActor != None && targetActor != sourceActor && !targetActor.IsDead()
            Return targetActor
        EndIf
        index += 1
    EndWhile
    Return None
EndFunction

Bool Function IsIntactObjectTarget(ObjectReference akTarget)
    Return akTarget != None && (akTarget as Actor) == None && !akTarget.IsDisabled() && !akTarget.IsDestroyed()
EndFunction

Bool Function HasIntactPreferredObject()
    Int index = 0
    While PreferredTargets != None && index < PreferredTargets.Length
        If PreferredTargets[index] != None && IsIntactObjectTarget(PreferredTargets[index].GetReference())
            Return True
        EndIf
        index += 1
    EndWhile

    index = 0
    While PreferredTargetCollection != None && index < PreferredTargetCollection.GetCount()
        If IsIntactObjectTarget(PreferredTargetCollection.GetAt(index))
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

Function DamagePreferredObjectTargets()
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsRunning()
        Return
    EndIf

    Int index = 0
    While PreferredTargets != None && index < PreferredTargets.Length
        If PreferredTargets[index] != None
            DamageObjectTarget(PreferredTargets[index].GetReference())
        EndIf
        index += 1
    EndWhile

    index = 0
    While PreferredTargetCollection != None && index < PreferredTargetCollection.GetCount()
        DamageObjectTarget(PreferredTargetCollection.GetAt(index))
        index += 1
    EndWhile

    If CountLivingMembers() > 0 && HasIntactPreferredObject()
        StartTimer(2.0, 7701)
    EndIf
EndFunction

Function DamageObjectTarget(ObjectReference akTarget)
    If !IsIntactObjectTarget(akTarget) || !akTarget.Is3DLoaded()
        Return
    EndIf

    ; FO76 caps machine damage with the destructible DPS limit (50 on the event consoles); four attackers at 5 DPS stays under it.
    Int attackers = 0
    Int index = 0
    While index < GetCount() && attackers < 4
        Actor attacker = GetAt(index) as Actor
        If attacker != None && !attacker.IsDead() && attacker.Is3DLoaded() && attacker.GetDistance(akTarget) <= 1024.0
            attackers += 1
        EndIf
        index += 1
    EndWhile
    If attackers > 0
        akTarget.DamageObject(10.0 * attackers)
    EndIf
EndFunction

Int Function CountLivingMembers()
    Int living = 0
    Int index = 0
    While index < GetCount()
        Actor member = GetAt(index) as Actor
        If member != None && !member.IsDead()
            living += 1
        EndIf
        index += 1
    EndWhile
    Return living
EndFunction
