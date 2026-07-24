; TODO

Event OnCombatStateChanged(Actor akTarget, int aeCombatState)
    Actor raRaRef = GetActorReference()
    If raRaRef == None
        Return
    EndIf

    If aeCombatState > 0
        If AnimArchetypeScared != None
            raRaRef.ChangeAnimArchetype(AnimArchetypeScared)
        EndIf
    ElseIf AnimArchetypeFriendly != None
        raRaRef.ChangeAnimArchetype(AnimArchetypeFriendly)
    EndIf
    raRaRef.EvaluatePackage()
EndEvent
