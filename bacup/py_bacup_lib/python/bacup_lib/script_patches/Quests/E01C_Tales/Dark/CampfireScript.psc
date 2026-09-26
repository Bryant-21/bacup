Event OnAliasInit()
    OwningQuest = GetOwningQuest()
    QS = OwningQuest as Quests:E01C_Tales:Dark:QuestScript
    CancelTimer(CampfireTimerID)
    CurrReductionPercent = 0.0
    AnimScript = None
    SetAnimState("lit")
EndEvent

Event OnAliasShutdown()
    StopDrain()
    SetAnimState("Off")
EndEvent

Event OnActivate(ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If akActionRef != playerRef || QS == None || OwningQuest == None || E01C_Tales_Dark_DryKindling == None
        Return
    EndIf
    If !OwningQuest.IsStageDone(900) || OwningQuest.IsStageDone(9000) || OwningQuest.IsStageDone(QS.Stage_CampfireWentOut) || CurrReductionPercent <= 0.0
        Return
    EndIf
    If playerRef.GetItemCount(E01C_Tales_Dark_DryKindling) < 1
        Return
    EndIf
    playerRef.RemoveItem(E01C_Tales_Dark_DryKindling, 1, True)
    QS.AddKindlingToCampfire()
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != CampfireTimerID || QS == None || OwningQuest == None || CurrReductionPercent <= 0.0
        Return
    EndIf
    If !OwningQuest.IsRunning() || OwningQuest.IsStageDone(9000) || OwningQuest.IsStageDone(QS.Stage_CampfireWentOut)
        StopDrain()
        Return
    EndIf
    QS.CampfireCurrPercent = QS.CampfireCurrPercent - CurrReductionPercent
    QS.UpdateCampfireProgressBar()
    If CurrReductionPercent > 0.0 && QS.CampfireCurrPercent > 0.0
        ScheduleDrainTick()
    EndIf
EndEvent

Function SetAnimState(String asState)
    If AnimScript == None
        ObjectReference campfireRef = GetReference()
        If campfireRef != None
            AnimScript = campfireRef as Quests:E01C_Tales:CampfireRef
        EndIf
    EndIf
    If AnimScript != None && AnimScript.GetState() != asState
        AnimScript.GoToState(asState)
    EndIf
EndFunction

Function StartDrain()
    If OwningQuest == None
        OwningQuest = GetOwningQuest()
        QS = OwningQuest as Quests:E01C_Tales:Dark:QuestScript
    EndIf
    ; Stage 10 is the source's debug switch that disables the campfire mechanic.
    If QS == None || OwningQuest.IsStageDone(10)
        Return
    EndIf
    QS.CampfireCurrPercent = QS.StartPercent
    CurrReductionPercent = QS.ReductionPercentStart
    QS.UpdateCampfireProgressBar()
    ScheduleDrainTick()
EndFunction

Function ScheduleDrainTick()
    Float interval = 1.0
    If QS != None && QS.ReductionTime > 1
        interval = QS.ReductionTime as Float
    EndIf
    CancelTimer(CampfireTimerID)
    StartTimer(interval, CampfireTimerID)
EndFunction

Function SetReductionPercent(Float afPercent)
    If CurrReductionPercent > 0.0 && afPercent > 0.0
        CurrReductionPercent = afPercent
    EndIf
EndFunction

Function StopDrain()
    CancelTimer(CampfireTimerID)
    CurrReductionPercent = 0.0
EndFunction

Function ExtinguishCampfire()
    SetAnimState("Off")
EndFunction

Function UpdateFireState(Float afPercent, Float afLowWarning)
    String desiredState = "Off"
    If afPercent >= 1.0
        desiredState = "blazing"
    ElseIf afPercent > afLowWarning
        desiredState = "lit"
    ElseIf afPercent > 0.0
        desiredState = "embers"
    EndIf
    If desiredState == "embers" && AnimScript != None && E01C_Tales_Dark_FireWarning != None
        String currentState = AnimScript.GetState()
        If currentState == "lit" || currentState == "blazing"
            E01C_Tales_Dark_FireWarning.Show()
        EndIf
    EndIf
    SetAnimState(desiredState)
EndFunction
