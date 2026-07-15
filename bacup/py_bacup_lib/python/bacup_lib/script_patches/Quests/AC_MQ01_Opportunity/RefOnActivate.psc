Bool Function IsAllowedActivator(ObjectReference akActionRef)
    If akActionRef == None
        Return False
    EndIf

    Bool hasFilters = False
    Int filterIndex = 0
    If ActivatedByReferences != None
        hasFilters = ActivatedByReferences.Length > 0
        While filterIndex < ActivatedByReferences.Length
            If ActivatedByReferences[filterIndex] == akActionRef
                Return True
            EndIf
            filterIndex += 1
        EndWhile
    EndIf

    Actor activatingActor = akActionRef as Actor
    filterIndex = 0
    If ActivatedByFactions != None
        hasFilters = hasFilters || ActivatedByFactions.Length > 0
        While activatingActor != None && filterIndex < ActivatedByFactions.Length
            If ActivatedByFactions[filterIndex] != None && activatingActor.IsInFaction(ActivatedByFactions[filterIndex])
                Return True
            EndIf
            filterIndex += 1
        EndWhile
    EndIf

    If hasFilters
        Return False
    EndIf
    Return akActionRef == Game.GetPlayer()
EndFunction

Event OnActivate(ObjectReference akActionRef)
    If !IsAllowedActivator(akActionRef) || MyQuest == None
        Return
    EndIf

    Actor activatingActor = akActionRef as Actor
    If activatingActor == Game.GetPlayer()
        If BlockWhilePlayerIsInCombat && activatingActor.IsInCombat()
            Return
        EndIf
        If BlockWhilePlayerIsSitting && activatingActor.GetSitState() != 0
            Return
        EndIf
        If BlockWhilePlayerIsInPowerArmor && activatingActor.IsInPowerArmor()
            Return
        EndIf
    EndIf

    If PrereqStage > 0 && !MyQuest.IsStageDone(PrereqStage)
        Return
    EndIf
    If TurnOffStage > 0 && MyQuest.IsStageDone(TurnOffStage)
        Return
    EndIf

    MyQuest.SetStage(StageToSet)
EndEvent
