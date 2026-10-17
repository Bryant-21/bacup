; Whitespring Refuge dining room. A diner taking a dining chair (Furn_Chairs) gets the nearest free waiter from
; Actors_Waiters: linking the waiter to the chair with LC060_DiningRoomWaiterLink drives
; LC060_WhitespringVendor_DiningRoom_WaiterTravelPackage (HasKeywordLinkedRef). On arrival the waiter greets the
; diner with the LC060_WhitespringDiningRoomWaiterDialogueKeyword subtype and is free again; the seat stays
; served until the diner stands up.
Event OnQuestInit()
    diningRoomAssignments = New DiningRoomData[0]
    lock_DiningRoomAssignments = False
EndEvent

Function AssignWaiter(Actor akDiner, ObjectReference akChair)
    If akDiner == None || akChair == None || !LockAssignments()
        Return
    EndIf
    If diningRoomAssignments == None
        diningRoomAssignments = New DiningRoomData[0]
    EndIf
    If diningRoomAssignments.FindStruct("diner", akDiner) < 0
        DiningRoomData entry = New DiningRoomData
        entry.diner = akDiner
        entry.diningRoomChair = akChair
        entry.waiter = FindFreeWaiter(akChair)
        If entry.waiter != None
            entry.waiter.SetLinkedRef(akChair, LC060_DiningRoomWaiterLink)
            entry.waiter.EvaluatePackage()
        EndIf
        diningRoomAssignments.Add(entry)
    EndIf
    lock_DiningRoomAssignments = False
    StartTimer(1.0, 1)
EndFunction

Function ReleaseDiner(Actor akDiner)
    If akDiner == None || !LockAssignments()
        Return
    EndIf
    Int index = diningRoomAssignments.FindStruct("diner", akDiner)
    If index >= 0
        FreeWaiter(diningRoomAssignments[index])
        diningRoomAssignments.Remove(index)
    EndIf
    lock_DiningRoomAssignments = False
EndFunction

Event OnTimer(Int aiTimerID)
    If !LockAssignments()
        StartTimer(1.0, 1)
        Return
    EndIf
    Int index = diningRoomAssignments.Length - 1
    While index >= 0
        DiningRoomData entry = diningRoomAssignments[index]
        If entry.diner == None || entry.diner.IsDead() || entry.diner.GetSitState() == 0
            FreeWaiter(entry)
            diningRoomAssignments.Remove(index)
        ElseIf entry.waiter == None
            entry.waiter = FindFreeWaiter(entry.diningRoomChair)
            If entry.waiter != None
                entry.waiter.SetLinkedRef(entry.diningRoomChair, LC060_DiningRoomWaiterLink)
                entry.waiter.EvaluatePackage()
            EndIf
        ElseIf entry.waiter.IsDead()
            FreeWaiter(entry)
        ElseIf entry.waiter.GetDistance(entry.diningRoomChair) <= CONST_WaiterTargetDistance * 2
            entry.waiter.SayCustom(LC060_WhitespringDiningRoomWaiterDialogueKeyword, entry.diner, False, entry.diner)
            FreeWaiter(entry)
        EndIf
        index -= 1
    EndWhile
    Bool waiting = diningRoomAssignments.Length > 0
    lock_DiningRoomAssignments = False
    If waiting
        StartTimer(1.0, 1)
    EndIf
EndEvent

Bool Function LockAssignments()
    Int attempts = 0
    While lock_DiningRoomAssignments && attempts < 50
        Utility.Wait(0.1)
        attempts += 1
    EndWhile
    If lock_DiningRoomAssignments
        Return False
    EndIf
    lock_DiningRoomAssignments = True
    If diningRoomAssignments == None
        diningRoomAssignments = New DiningRoomData[0]
    EndIf
    Return True
EndFunction

Function FreeWaiter(DiningRoomData akEntry)
    If akEntry.waiter != None
        akEntry.waiter.SetLinkedRef(None, LC060_DiningRoomWaiterLink)
        akEntry.waiter.EvaluatePackage()
        akEntry.waiter = None
    EndIf
EndFunction

Actor Function FindFreeWaiter(ObjectReference akChair)
    If WhitespringDiningRoomWaiters == None
        Return None
    EndIf
    Actor nearest = None
    Float nearestDistance = 0.0
    Int index = WhitespringDiningRoomWaiters.GetCount() - 1
    While index >= 0
        Actor candidate = WhitespringDiningRoomWaiters.GetAt(index) as Actor
        If candidate != None && !candidate.IsDead() && !candidate.IsDisabled() && diningRoomAssignments.FindStruct("waiter", candidate) < 0
            Float distance = candidate.GetDistance(akChair)
            If nearest == None || distance < nearestDistance
                nearest = candidate
                nearestDistance = distance
            EndIf
        EndIf
        index -= 1
    EndWhile
    Return nearest
EndFunction
