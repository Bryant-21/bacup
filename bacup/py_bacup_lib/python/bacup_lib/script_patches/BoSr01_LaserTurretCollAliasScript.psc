; Timer 1 re-checks broken turrets, because an actor repair raises no destruction event.
Event OnAliasInit()
    OwningQuest = GetOwningQuest()
    If OwningQuest != None
        RegisterForRemoteEvent(OwningQuest, "OnStageSet")
    EndIf
    RefreshRepairObjective()
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    RefreshRepairObjective()
EndEvent

Event OnLoad(ObjectReference akSenderRef)
    RefreshRepairObjective()
EndEvent

Event OnDeath(ObjectReference akSenderRef, Actor akKiller)
    RefreshRepairObjective()
EndEvent

Event OnDestructionStageChanged(ObjectReference akSenderRef, Int aiOldStage, Int aiCurrentStage)
    RefreshRepairObjective()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID == 1
        RefreshRepairObjective()
    EndIf
EndEvent

Event OnAliasShutdown()
    CancelTimer(1)
    UnregisterForAllEvents()
EndEvent

Bool Function RepairWindowOpen()
    Return OwningQuest != None && OwningQuest.IsRunning() && OwningQuest.IsStageDone(StageToDisplay) && !OwningQuest.IsStageDone(StageToHide)
EndFunction

Bool Function AnyTurretNeedsRepair()
    Int index = 0
    While index < GetCount()
        If DefaultAliasOnObjectRepaired.NeedsRepair(GetAt(index))
            Return True
        EndIf
        index += 1
    EndWhile
    Return False
EndFunction

Function RefreshRepairObjective()
    CancelTimer(1)
    If OwningQuest == None
        OwningQuest = GetOwningQuest()
    EndIf
    If OwningQuest == None
        Return
    EndIf
    Bool objectiveOpen = OwningQuest.IsObjectiveDisplayed(RepairObjective) && !OwningQuest.IsObjectiveCompleted(RepairObjective)
    If !RepairWindowOpen()
        If objectiveOpen
            OwningQuest.SetObjectiveDisplayed(RepairObjective, False)
        EndIf
        Return
    EndIf
    If AnyTurretNeedsRepair()
        If !objectiveOpen
            OwningQuest.SetObjectiveCompleted(RepairObjective, False)
            OwningQuest.SetObjectiveDisplayed(RepairObjective, True)
        EndIf
        StartTimer(5.0, 1)
    ElseIf objectiveOpen
        OwningQuest.SetObjectiveCompleted(RepairObjective, True)
    EndIf
EndFunction
