; Between StartStage and EndStage every reference in the collection moves the owning progress bar by
; ModPercentageContinuous each TimerSeconds (times the count when MultiplyByCount); clearing one removes it.
Event OnAliasInit()
    ProgressBarQuest = GetOwningQuest()
    ProgressBar = ProgressBarQuest as Quests:_Default:ProgressBar:MasterScript
    If ProgressBarQuest != None
        RegisterForRemoteEvent(ProgressBarQuest, "OnStageSet")
    EndIf
    If ModifierActive()
        StartTimer(TimerSeconds, 1)
    EndIf
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If auiStageID == StartStage
        StartTimer(TimerSeconds, 1)
    EndIf
EndEvent

Event OnAliasShutdown()
    CancelTimer(1)
    If ProgressBarQuest != None
        UnregisterForRemoteEvent(ProgressBarQuest, "OnStageSet")
    EndIf
EndEvent

Bool Function ModifierActive()
    If ProgressBarQuest == None || !ProgressBarQuest.IsRunning()
        Return False
    EndIf
    If StartStage >= 0 && !ProgressBarQuest.IsStageDone(StartStage)
        Return False
    EndIf
    Return EndStage < 0 || !ProgressBarQuest.IsStageDone(EndStage)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID != 1 || !ModifierActive()
        Return
    EndIf
    Int active = 0
    Int index = 0
    While index < GetCount()
        ObjectReference target = GetAt(index)
        If target != None && target.IsEnabled()
            active += 1
        EndIf
        index += 1
    EndWhile
    If active > 0 && ProgressBar != None && ModPercentageContinuous != 0.0
        Float amount = ModPercentageContinuous
        If MultiplyByCount
            amount *= active
        EndIf
        ProgressBar.ModProgress(amount)
    EndIf
    StartTimer(TimerSeconds, 1)
EndEvent

Event OnActivate(ObjectReference akSenderRef, ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || akSenderRef == None || !ModifierActive()
        Return
    EndIf
    If ProgressBar != None && ModPercentageOnActivate != 0.0
        ProgressBar.ModProgress(ModPercentageOnActivate)
    EndIf
    If SuccessSound != None
        SuccessSound.Play(akSenderRef)
    EndIf
    If RemoveFromAliasOnActivate
        RemoveRef(akSenderRef)
    EndIf
    If DisableRefOnActivate
        akSenderRef.Disable()
    EndIf
    If DeleteRefOnActivate
        RemoveRef(akSenderRef)
        akSenderRef.Disable()
        akSenderRef.Delete()
    EndIf
EndEvent
