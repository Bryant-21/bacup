Event OnQuestInit()
    SetupWasteDispensers()
    RegisterAliasActivation(0)
    RegisterAliasActivation(14)
EndEvent

Function RegisterAliasActivation(Int aiAliasID)
    ReferenceAlias targetAlias = GetAlias(aiAliasID) as ReferenceAlias
    If targetAlias != None && targetAlias.GetReference() != None
        RegisterForRemoteEvent(targetAlias.GetReference(), "OnActivate")
    EndIf
EndFunction

Bool Function IsPompy(ObjectReference akSender)
    Form pompyBase = Game.GetFormFromFile(0x0047F165, "SeventySix.esm")
    Return akSender != None && pompyBase != None && akSender.GetBaseObject() == pompyBase
EndFunction

Bool Function IsDailyAvailableToday(Actor akPlayer)
    ActorValue dailyTimestamp = Game.GetFormFromFile(0x0045E3E5, "SeventySix.esm") as ActorValue
    If akPlayer == None || dailyTimestamp == None
        Return false
    EndIf

    Float completedAt = akPlayer.GetValue(dailyTimestamp)
    Return completedAt <= 0.0 || (Utility.GetCurrentGameTime() as Int) > (completedAt as Int)
EndFunction

Function TryRestartDaily(Actor akPlayer)
    If !IsDailyAvailableToday(akPlayer)
        Return
    EndIf

    Quest dailyQuest = Game.GetFormFromFile(0x0045E3D2, "SeventySix.esm") as Quest
    Keyword startKeyword = Game.GetFormFromFile(0x0045E3EB, "SeventySix.esm") as Keyword
    If dailyQuest == None || startKeyword == None
        Return
    EndIf

    If dailyQuest.IsRunning()
        dailyQuest.Stop()
    EndIf
    If dailyQuest.IsCompleted() || dailyQuest.IsStopped()
        dailyQuest.Reset()
    EndIf
    startKeyword.SendStoryEventAndWait(None, akPlayer, akPlayer)
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    Actor playerRef = PlayerAlias.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    If playerRef == None || akActionRef != playerRef
        Return
    EndIf

    If IsPompy(akSender) && (IsStageDone(900) || IsStageDone(1000) || IsCompleted() || IsStopped())
        TryRestartDaily(playerRef)
        Return
    EndIf

    ReferenceAlias leaderAlias = GetAlias(0) as ReferenceAlias
    ReferenceAlias collectorAlias = GetAlias(14) as ReferenceAlias
    If leaderAlias != None && akSender == leaderAlias.GetReference()
        If !IsStageDone(200)
            SetStage(200)
        EndIf
    ElseIf collectorAlias != None && akSender == collectorAlias.GetReference()
        If IsStageDone(300) && !IsStageDone(1000)
            SetStage(1000)
        EndIf
    EndIf
EndEvent

Function SetupWasteDispensers()
    If ChosenGooPiles == None || ChosenGooPiles.GetCount() < 5
        Return
    EndIf

    BindDispenserAlias(ToxicWasteDispenser01, 0)
    BindDispenserAlias(ToxicWasteDispenser02, 1)
    BindDispenserAlias(ToxicWasteDispenser03, 2)
    BindDispenserAlias(ToxicWasteDispenser04, 3)
    BindDispenserAlias(ToxicWasteDispenser05, 4)
    dispenserLock = false
EndFunction

Function BindDispenserAlias(ReferenceAlias akDispenserAlias, Int aiCollectionIndex)
    ObjectReference wastePile = ChosenGooPiles.GetAt(aiCollectionIndex)
    If akDispenserAlias != None && wastePile != None
        akDispenserAlias.ForceRefTo(wastePile)
        akDispenserAlias.TryToEnableNoWait()
    EndIf
EndFunction

Function CollectWaste(ObjectReference akDispenser, Int aiStageToSet)
    If dispenserLock || akDispenser == None || IsStageDone(aiStageToSet)
        Return
    EndIf

    Actor playerRef = PlayerAlias.GetActorReference()
    If playerRef == None
        Return
    EndIf

    dispenserLock = true
    playerRef.AddItem(D01C_ToxicWaste, 1, false)
    SetStage(aiStageToSet)
    akDispenser.Disable()
    dispenserLock = false
EndFunction

Function CleanupWasteDispensers()
    dispenserLock = true
    ToxicWasteDispenser01.TryToDisableNoWait()
    ToxicWasteDispenser02.TryToDisableNoWait()
    ToxicWasteDispenser03.TryToDisableNoWait()
    ToxicWasteDispenser04.TryToDisableNoWait()
    ToxicWasteDispenser05.TryToDisableNoWait()
    ToxicWasteDispenser01.Clear()
    ToxicWasteDispenser02.Clear()
    ToxicWasteDispenser03.Clear()
    ToxicWasteDispenser04.Clear()
    ToxicWasteDispenser05.Clear()
EndFunction
