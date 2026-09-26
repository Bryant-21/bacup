Quest Function OwningQuest()
    If myQuest == None
        myQuest = GetOwningQuest()
    EndIf
    Return myQuest
EndFunction

Bool Function SprayRunStillActive()
    Quest owningQuest = OwningQuest()
    If owningQuest == None || !owningQuest.IsRunning()
        Return False
    EndIf
    Return !owningQuest.IsStageDone(55) && !owningQuest.IsStageDone(90) && !owningQuest.IsStageDone(97) && !owningQuest.IsStageDone(99)
EndFunction

Int Function CurrentSprayObjective()
    Quest owningQuest = OwningQuest()
    If owningQuest == None || StageObjectives == None
        Return -1
    EndIf
    Int row = StageObjectives.Length - 1
    While row >= 0
        Int stage = StageObjectives[row].Stage
        Int objective = StageObjectives[row].Objective
        If owningQuest.IsStageDone(stage) && !owningQuest.IsObjectiveCompleted(objective)
            Return objective
        EndIf
        row -= 1
    EndWhile
    Return -1
EndFunction

Float Function RepairWindowSeconds()
    Float seconds = BleedTimer as Float
    If seconds < 1.0
        seconds = 1.0
    EndIf
    Return seconds
EndFunction

Function BeginRepairWindow()
    Quest owningQuest = OwningQuest()
    If owningQuest == None || !SprayRunStillActive()
        Return
    EndIf
    owningQuest.SetObjectiveDisplayed(600, True, True)
    CancelTimer(46952)
    StartTimer(1.0, 46952)
    CancelTimer(46953)
    StartTimer(RepairWindowSeconds(), 46953)
EndFunction

Function CloseRepairWindow()
    CancelTimer(46952)
    CancelTimer(46953)
    Quest owningQuest = OwningQuest()
    If owningQuest == None
        Return
    EndIf
    owningQuest.SetObjectiveDisplayed(600, False)
    Int sprayObjective = CurrentSprayObjective()
    If sprayObjective >= 0 && SprayRunStillActive()
        owningQuest.SetObjectiveDisplayed(sprayObjective, True, True)
    EndIf
EndFunction

Function RepairPharmabot()
    Actor botRef = GetActorReference()
    If botRef != None
        botRef.ResetHealthAndLimbs()
    EndIf
    CloseRepairWindow()
EndFunction

Bool Function PlayerCanReachPharmabot()
    Actor botRef = GetActorReference()
    Actor playerRef = Game.GetPlayer()
    If botRef == None || playerRef == None
        Return False
    EndIf
    Return botRef.Is3DLoaded() && playerRef.GetDistance(botRef) <= 256.0
EndFunction

Event OnAliasInit()
    myQuest = GetOwningQuest()
    mySceneInstance = myScene
EndEvent

Event OnEnterBleedout()
    BeginRepairWindow()
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf
    Actor botRef = GetActorReference()
    If botRef != None && botRef.IsBleedingOut()
        RepairPharmabot()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    Actor botRef = GetActorReference()
    If botRef == None
        CancelTimer(46952)
        CancelTimer(46953)
        Return
    EndIf
    If aiTimerID == 46952
        If !botRef.IsBleedingOut()
            CloseRepairWindow()
        ElseIf PlayerCanReachPharmabot()
            RepairPharmabot()
        Else
            StartTimer(1.0, 46952)
        EndIf
    ElseIf aiTimerID == 46953
        CancelTimer(46952)
        If !botRef.IsBleedingOut()
            CloseRepairWindow()
        ElseIf SprayRunStillActive()
            Quest owningQuest = OwningQuest()
            If owningQuest != None
                owningQuest.SetStage(90)
            EndIf
        EndIf
    EndIf
EndEvent

Event OnDying(Actor akKiller)
    Quest owningQuest = OwningQuest()
    If owningQuest != None && SprayRunStillActive()
        owningQuest.SetStage(90)
    EndIf
EndEvent

Event OnAliasShutdown()
    CancelTimer(46952)
    CancelTimer(46953)
EndEvent
