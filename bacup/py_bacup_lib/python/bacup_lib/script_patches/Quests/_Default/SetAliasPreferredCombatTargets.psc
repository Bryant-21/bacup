Event OnAliasInit()
    If !UseOnLoadTiming
        ApplyPreferredCombatTarget()
    EndIf
EndEvent

Event OnLoad()
    If UseOnLoadTiming
        ApplyPreferredCombatTarget()
    EndIf
EndEvent

Event OnWorkshopObjectRepaired(ObjectReference akReference)
    If SetTargetOnRepaired
        ApplyPreferredCombatTarget()
    EndIf
EndEvent

Function ApplyPreferredCombatTarget()
    Actor sourceActor = GetActorReference()
    If sourceActor == None || sourceActor.IsDead()
        Return
    EndIf

    Actor targetActor = FindPreferredCombatTarget(sourceActor)
    If targetActor == None
        Return
    EndIf

    If PersistTargets && PersistCombatTargetsAfterCombatEndKeyword != None && !sourceActor.HasKeyword(PersistCombatTargetsAfterCombatEndKeyword)
        sourceActor.AddKeyword(PersistCombatTargetsAfterCombatEndKeyword)
    EndIf
    If StartCombat
        sourceActor.StartCombat(targetActor, True)
    EndIf
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
