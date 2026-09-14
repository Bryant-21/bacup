Event OnQuestInit()
    PlayerRef = Alias_Player.GetActorReference()
    ReferenceAlias leaderAlias = GetAlias(1) as ReferenceAlias
    If leaderAlias != None && leaderAlias.GetReference() != None
        RegisterForRemoteEvent(leaderAlias.GetReference(), "OnActivate")
    EndIf
EndEvent

Bool Function IsTreadly(ObjectReference akSender)
    Form treadlyBase = Game.GetFormFromFile(0x0047F168, "SeventySix.esm")
    Return akSender != None && treadlyBase != None && akSender.GetBaseObject() == treadlyBase
EndFunction

Bool Function IsDailyAvailableToday(Actor akPlayer)
    ActorValue dailyTimestamp = Game.GetFormFromFile(0x0046C95A, "SeventySix.esm") as ActorValue
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

    Quest dailyQuest = Game.GetFormFromFile(0x0045E3D3, "SeventySix.esm") as Quest
    Keyword startKeyword = Game.GetFormFromFile(0x0046A629, "SeventySix.esm") as Keyword
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
    If PlayerRef == None
        PlayerRef = Game.GetPlayer()
    EndIf
    If PlayerRef == None || akActionRef != PlayerRef
        Return
    EndIf

    If IsTreadly(akSender) && (IsStageDone(900) || IsStageDone(1000) || IsCompleted() || IsStopped())
        TryRestartDaily(PlayerRef)
        Return
    EndIf

    ReferenceAlias leaderAlias = GetAlias(1) as ReferenceAlias
    If leaderAlias == None || akSender != leaderAlias.GetReference()
        Return
    EndIf

    If IsStageDone(StageCollected)
        If !IsStageDone(1000)
            SetStage(1000)
        EndIf
    ElseIf !IsStageDone(200)
        SetStage(200)
    EndIf
EndEvent

Function CheckExistingParts()
    If PlayerRef == None
        PlayerRef = Alias_Player.GetActorReference()
    EndIf
    If PlayerRef == None || ItemsNeeded == None
        Return
    EndIf

    Int partIndex = 0
    While partIndex < ItemsNeeded.Length
        If ItemsNeeded[partIndex].Item != None && PlayerRef.GetItemCount(ItemsNeeded[partIndex].Item) > 0
            SetStage(ItemsNeeded[partIndex].Stage)
        EndIf
        partIndex += 1
    EndWhile
    CheckAllPartsCollected()
EndFunction

Function CheckAllPartsCollected()
    If ItemsNeeded == None || ItemsNeeded.Length == 0
        Return
    EndIf

    Int partIndex = 0
    While partIndex < ItemsNeeded.Length
        If !IsStageDone(ItemsNeeded[partIndex].Stage)
            Return
        EndIf
        partIndex += 1
    EndWhile

    If !IsStageDone(StageCollected)
        SetStage(StageCollected)
    EndIf
EndFunction
