; EnableAllInit shows every reference at once (leftovers). Otherwise the references start hidden and, from
; StartStage, one more appears every PeriodicEnableTime (brush fires) while fewer than MaxEnabled are burning.
Event OnAliasInit()
    Int index = 0
    While index < GetCount()
        ObjectReference target = GetAt(index)
        If target != None
            If EnableAllInit
                target.Enable()
            Else
                target.Disable()
            EndIf
        EndIf
        index += 1
    EndWhile
    If EnableAllInit || PeriodicEnableTime <= 0.0
        Return
    EndIf
    Quest owner = GetOwningQuest()
    If owner != None
        RegisterForRemoteEvent(owner, "OnStageSet")
        If StartStage < 0 || owner.IsStageDone(StartStage)
            StartTimer(PeriodicEnableTime, 1)
        EndIf
    EndIf
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If auiStageID == StartStage
        StartTimer(PeriodicEnableTime, 1)
    EndIf
EndEvent

Event OnAliasShutdown()
    CancelTimer(1)
    If GetOwningQuest() != None
        UnregisterForRemoteEvent(GetOwningQuest(), "OnStageSet")
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    Quest owner = GetOwningQuest()
    If aiTimerID != 1 || owner == None || !owner.IsRunning() || (EndStage >= 0 && owner.IsStageDone(EndStage))
        Return
    EndIf
    Int burning = 0
    If AliasToFillWhenEnabled != None
        burning = AliasToFillWhenEnabled.GetCount()
    EndIf
    If MaxEnabled < 0 || burning < MaxEnabled
        EnableRandomHidden()
    EndIf
    StartTimer(PeriodicEnableTime, 1)
EndEvent

Function EnableRandomHidden()
    Int hiddenCount = 0
    Int index = 0
    While index < GetCount()
        If GetAt(index) != None && GetAt(index).IsDisabled()
            hiddenCount += 1
        EndIf
        index += 1
    EndWhile
    If hiddenCount == 0
        Return
    EndIf
    Int pick = Utility.RandomInt(0, hiddenCount - 1)
    index = 0
    While index < GetCount()
        ObjectReference target = GetAt(index)
        If target != None && target.IsDisabled()
            If pick == 0
                target.Enable()
                If AliasToFillWhenEnabled != None
                    AliasToFillWhenEnabled.AddRef(target)
                EndIf
                Return
            EndIf
            pick -= 1
        EndIf
        index += 1
    EndWhile
EndFunction
