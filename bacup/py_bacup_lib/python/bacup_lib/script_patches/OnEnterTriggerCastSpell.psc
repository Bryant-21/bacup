Bool Function IsValidTrigger(ObjectReference akActionRef)
    If akActionRef == None
        Return False
    EndIf

    If PlayerOnly || PlayerTriggerOnly
        Return akActionRef == Game.GetPlayer()
    EndIf

    Bool hasFilters = False
    Int filterIndex = 0
    If TriggeredByReferences != None
        hasFilters = TriggeredByReferences.Length > 0
        While filterIndex < TriggeredByReferences.Length
            If TriggeredByReferences[filterIndex] == akActionRef
                Return True
            EndIf
            filterIndex += 1
        EndWhile
    EndIf

    filterIndex = 0
    If TriggeredByAliases != None
        hasFilters = hasFilters || TriggeredByAliases.Length > 0
        While filterIndex < TriggeredByAliases.Length
            If TriggeredByAliases[filterIndex] != None && TriggeredByAliases[filterIndex].GetReference() == akActionRef
                Return True
            EndIf
            filterIndex += 1
        EndWhile
    EndIf

    Actor triggeringActor = akActionRef as Actor
    filterIndex = 0
    If TriggeredByFactions != None
        hasFilters = hasFilters || TriggeredByFactions.Length > 0
        While triggeringActor != None && filterIndex < TriggeredByFactions.Length
            If TriggeredByFactions[filterIndex] != None && triggeringActor.IsInFaction(TriggeredByFactions[filterIndex])
                Return True
            EndIf
            filterIndex += 1
        EndWhile
    EndIf

    Return !hasFilters
EndFunction

Event OnTriggerEnter(ObjectReference akActionRef)
    If SpinLock || !IsValidTrigger(akActionRef)
        Return
    EndIf

    SpinLock = True
    If SelfCast
        SpellToCast.Cast(akActionRef, akActionRef)
    Else
        SpellToCast.Cast(Self, akActionRef)
    EndIf
    SpinLock = False
EndEvent
