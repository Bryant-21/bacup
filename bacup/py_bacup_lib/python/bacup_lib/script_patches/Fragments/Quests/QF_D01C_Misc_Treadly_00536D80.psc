Event OnQuestInit()
    ReferenceAlias leaderAlias = GetAlias(0) as ReferenceAlias
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

Function TryStartDaily(Actor akPlayer)
    If !IsDailyAvailableToday(akPlayer)
        Return
    EndIf

    Quest dailyQuest = Game.GetFormFromFile(0x0045E3D3, "SeventySix.esm") as Quest
    Keyword startKeyword = Game.GetFormFromFile(0x0046A629, "SeventySix.esm") as Keyword
    If dailyQuest == None || startKeyword == None || dailyQuest.IsRunning()
        Return
    EndIf

    If dailyQuest.IsCompleted() || dailyQuest.IsStopped()
        dailyQuest.Reset()
    EndIf
    startKeyword.SendStoryEventAndWait(None, akPlayer, akPlayer)
EndFunction

Event ObjectReference.OnActivate(ObjectReference akSender, ObjectReference akActionRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || akActionRef != playerRef || !IsTreadly(akSender)
        Return
    EndIf

    Quest dailyQuest = Game.GetFormFromFile(0x0045E3D3, "SeventySix.esm") as Quest
    If dailyQuest != None && dailyQuest.IsRunning()
        If IsRunning() && !IsStageDone(9000)
            SetStage(9000)
        EndIf
    ElseIf IsRunning() && !IsStageDone(9000) && IsDailyAvailableToday(playerRef)
        SetStage(9000)
    Else
        TryStartDaily(playerRef)
    EndIf
EndEvent

Function Fragment_Stage_0100_Item_00()
    GetOwningQuest().SetObjectiveDisplayed(10)
EndFunction

Function Fragment_Stage_9000_Item_00()
    Quest owningQuest = GetOwningQuest()
    owningQuest.SetObjectiveCompleted(10)
    TryStartDaily(Game.GetPlayer())
EndFunction
