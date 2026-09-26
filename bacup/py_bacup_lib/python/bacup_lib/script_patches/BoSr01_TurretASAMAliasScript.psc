; Timer 1 re-checks a broken turret, because an actor repair raises no destruction event.
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

Event OnLoad()
    RefreshRepairObjective()
EndEvent

Event OnDeath(Actor akKiller)
    RefreshRepairObjective()
EndEvent

Event OnDestructionStageChanged(Int aiOldStage, Int aiCurrentStage)
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

Function RefreshRepairObjective()
    CancelTimer(1)
    If OwningQuest == None
        OwningQuest = GetOwningQuest()
    EndIf
    SelfRef = GetReference()
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
    If SelfRef == None
        Return
    EndIf
    If DefaultAliasOnObjectRepaired.NeedsRepair(SelfRef)
        If !objectiveOpen
            OwningQuest.SetObjectiveCompleted(RepairObjective, False)
            OwningQuest.SetObjectiveDisplayed(RepairObjective, True)
        EndIf
        StartTimer(5.0, 1)
    ElseIf objectiveOpen
        OwningQuest.SetObjectiveCompleted(RepairObjective, True)
    EndIf
EndFunction
