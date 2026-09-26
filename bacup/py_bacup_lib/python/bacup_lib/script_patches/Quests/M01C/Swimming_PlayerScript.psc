; Buoys are reached by distance (fTargetDistance) once the timed course starts at 200.
Function BeginSwimTest()
    Quest owningQuest = GetOwningQuest()
    ObjectReference swimmerRef = GetReference()
    If owningQuest == None || swimmerRef == None || BuoyList == None
        Return
    EndIf
    BuoysCount_Current = 0
    Int i = 0
    While i < BuoyList.Length
        If owningQuest.IsStageDone(BuoyList[i].StageToSet)
            BuoysCount_Current += 1
        ElseIf BuoyList[i].BuoyNumber && BuoyList[i].BuoyNumber.GetReference()
            RegisterForDistanceLessThanEvent(swimmerRef, BuoyList[i].BuoyNumber.GetReference(), fTargetDistance)
        EndIf
        i += 1
    EndWhile
    PublishBuoyCount()
    If InstructionObjective > 0
        owningQuest.SetObjectiveCompleted(InstructionObjective, True)
    EndIf
    owningQuest.SetObjectiveDisplayed(20, True)
    If BuoyObjective > 0
        owningQuest.SetObjectiveDisplayed(BuoyObjective, True)
    EndIf
    CheckAllBuoys()
EndFunction

Event OnDistanceLessThan(ObjectReference akObj1, ObjectReference akObj2, Float afDistance)
    Quest owningQuest = GetOwningQuest()
    If owningQuest == None || !owningQuest.IsStageDone(SwimTest_StartStage) || owningQuest.IsStageDone(AllBuoyComplete_Stage)
        Return
    EndIf
    Int i = 0
    While i < BuoyList.Length
        ObjectReference buoyRef = None
        If BuoyList[i].BuoyNumber
            buoyRef = BuoyList[i].BuoyNumber.GetReference()
        EndIf
        If buoyRef && (buoyRef == akObj1 || buoyRef == akObj2) && !owningQuest.IsStageDone(BuoyList[i].StageToSet)
            BuoysCount_Current += 1
            owningQuest.SetStage(BuoyList[i].StageToSet)
            PublishBuoyCount()
            CheckAllBuoys()
            Return
        EndIf
        i += 1
    EndWhile
EndEvent

Function CheckAllBuoys()
    Quest owningQuest = GetOwningQuest()
    If BuoysCount_Current >= BuoyList.Length && !owningQuest.IsStageDone(AllBuoyComplete_Stage)
        owningQuest.SetStage(AllBuoyComplete_Stage)
    EndIf
EndFunction

Function PublishBuoyCount()
    B21:QuestVariables questVariables = GetOwningQuest() as B21:QuestVariables
    If questVariables
        questVariables.SetVariable("buoys_current", BuoysCount_Current as Float)
        questVariables.SetVariable("buoys_total", BuoyList.Length as Float)
    EndIf
EndFunction

Event OnAliasShutdown()
    ObjectReference swimmerRef = GetReference()
    If swimmerRef == None || BuoyList == None
        Return
    EndIf
    Int i = 0
    While i < BuoyList.Length
        If BuoyList[i].BuoyNumber && BuoyList[i].BuoyNumber.GetReference()
            UnregisterForDistanceEvents(swimmerRef, BuoyList[i].BuoyNumber.GetReference())
        EndIf
        i += 1
    EndWhile
EndEvent
