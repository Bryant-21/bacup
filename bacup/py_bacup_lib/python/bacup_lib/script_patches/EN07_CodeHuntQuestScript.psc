Actor Function GetLocalPlayer()
    If currentPlayer != None
        Actor aliasPlayer = currentPlayer.GetActorReference()
        If aliasPlayer != None
            Return aliasPlayer
        EndIf
    EndIf
    Return Game.GetPlayer()
EndFunction

Bool Function HasLocalCodePiece(Actor akPlayer, Int aiSiloGroupID)
    If akPlayer == None
        Return False
    EndIf
    Int firstFormID = 0x003DA648
    If aiSiloGroupID == 1
        firstFormID = 0x004DE233
    ElseIf aiSiloGroupID == 2
        firstFormID = 0x004DE23B
    EndIf
    Int i = 0
    While i < 8
        Form codePage = Game.GetFormFromFile(firstFormID + i, "SeventySix.esm")
        If codePage != None && akPlayer.GetItemCount(codePage) > 0
            Return True
        EndIf
        i += 1
    EndWhile
    Return False
EndFunction

Function BeginLocalHunt(Int aiTargetType, Int aiSiloGroupID, ObjectReference akTarget, ObjectReference akTarget02)
    If aiTargetType < 0 || aiTargetType > 1
        Return
    EndIf
    iTargetType = aiTargetType
    iSiloGroupID = aiSiloGroupID
    If Alias_Target != None && akTarget != None
        Alias_Target.ForceRefTo(akTarget)
    EndIf
    If Alias_Target02 != None && akTarget02 != None
        Alias_Target02.ForceRefTo(akTarget02)
    EndIf

    Actor player = GetLocalPlayer()
    If player != None
        player.SetValue(EN07_MQ_CodeHuntTargetType, aiTargetType as Float)
        player.SetValue(EN07_MQ_CodeHuntSiloID, aiSiloGroupID as Float)
        player.SetValue(EN07_MQ_CodeHuntActive, 1.0)
    EndIf
    If !IsStageDone(iStartStage)
        SetStage(iStartStage)
    Else
        HandleStage(iStartStage)
    EndIf
    StartTimer(1.0, 7101)
EndFunction

Function HandleStage(Int aiStage)
    Actor player = GetLocalPlayer()
    If aiStage == iStartStage
        If iTargetType == 1
            SetObjectiveDisplayed(LaunchCardObjective)
        Else
            SetObjectiveDisplayed(CodePageObjective)
        EndIf
    ElseIf aiStage == LaunchCardVertibotDeadStage
        SetObjectiveDisplayed(LaunchCardObjective, True, True)
    ElseIf aiStage == iSuccessStage
        If iTargetType == 1
            SetObjectiveCompleted(LaunchCardObjective)
            Quest introQuest = Game.GetFormFromFile(0x002D0F6B, "SeventySix.esm") as Quest
            If introQuest != None && !introQuest.IsStageDone(40)
                introQuest.SetStage(40)
            EndIf
        Else
            SetObjectiveCompleted(CodePageObjective)
            Quest introQuest = Game.GetFormFromFile(0x002D0F6B, "SeventySix.esm") as Quest
            If introQuest != None && !introQuest.IsStageDone(50)
                introQuest.SetStage(50)
            EndIf
        EndIf
        If player != None
            player.SetValue(EN07_MQ_CodeHuntActive, 0.0)
        EndIf
        SetStage(1000)
    ElseIf aiStage == iFailStage
        If iTargetType == 1
            SetObjectiveFailed(LaunchCardObjective)
        Else
            SetObjectiveFailed(CodePageObjective)
        EndIf
        If player != None
            player.SetValue(EN07_MQ_CodeHuntActive, 0.0)
        EndIf
        SetStage(1000)
    ElseIf aiStage == 1000
        If player != None
            player.SetValue(EN07_MQ_CodeHuntActive, 0.0)
        EndIf
        Stop()
    EndIf
EndFunction

Event OnQuestInit()
    Actor player = GetLocalPlayer()
    If player != None
        player.SetValue(EN07_MQ_CodeHuntActive, 1.0)
    EndIf
EndEvent

Event OnStoryScript(Keyword akKeyword, Location akLocation, ObjectReference akRef1, ObjectReference akRef2, Int aiValue1, Int aiValue2)
    BeginLocalHunt(aiValue1, aiValue2, akRef1, akRef2)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 7101 || !IsRunning() || IsStageDone(iSuccessStage) || IsStageDone(iFailStage)
        Return
    EndIf
    Actor player = GetLocalPlayer()
    Bool acquired = False
    If iTargetType == 1
        acquired = player != None && player.GetItemCount(Nuke_LaunchCard) > 0
    ElseIf iTargetType == 0
        acquired = HasLocalCodePiece(player, iSiloGroupID)
    EndIf
    If acquired
        SetStage(iSuccessStage)
    Else
        StartTimer(1.0, 7101)
    EndIf
EndEvent
