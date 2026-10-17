; From StartStage, Chally leaves a stinky pile every PooTimerSeconds, never more than MaxPoo at once.
Event OnAliasInit()
    Quest owner = GetOwningQuest()
    If owner != None
        RegisterForRemoteEvent(owner, "OnStageSet")
        If StartStage >= 0 && owner.IsStageDone(StartStage)
            StartTimer(PooTimerSeconds, 1)
        EndIf
    EndIf
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If auiStageID == StartStage
        StartTimer(PooTimerSeconds, 1)
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
    ChallyRef = GetReference()
    If ChallyRef != None && Alias_Activators_Poo != None && E02A_Meat_BBQ_ChallyStinkyPile != None && Alias_Activators_Poo.GetCount() < MaxPoo
        ObjectReference pile = ChallyRef.PlaceAtMe(E02A_Meat_BBQ_ChallyStinkyPile)
        If pile != None
            Alias_Activators_Poo.AddRef(pile)
        EndIf
    EndIf
    StartTimer(PooTimerSeconds, 1)
EndEvent
