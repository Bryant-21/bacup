Event OnQuestInit()
    myPlayer = PlayerAlias.GetActorReference()
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 301 && !IsStageDone(300)
        SetStage(300)
    ElseIf auiStageID == 304 && !IsStageDone(310)
        SetStage(310)
    ElseIf auiStageID == 310
        CancelTimer(1)
        StartTimer(1.0, 1)
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 1 || !IsRunning() || !IsStageDone(310) || IsStageDone(700)
        Return
    EndIf

    CheckRequiredQuests()
    If IsRunning() && !IsStageDone(700)
        StartTimer(5.0, 1)
    EndIf
EndEvent

Function CheckRequiredQuests()
    Bool allCompleted = Quests != None && Quests.Length > 0
    Int index = 0
    While Quests != None && index < Quests.Length
        RequiredQuest entry = Quests[index]
        Quest requiredQuest = entry.Quest_Required
        If requiredQuest == None
            allCompleted = false
        Else
            If requiredQuest.IsCompleted()
                If entry.StageToSet >= 0 && !IsStageDone(entry.StageToSet)
                    SetStage(entry.StageToSet)
                EndIf
            Else
                allCompleted = false
            EndIf
        EndIf
        index += 1
    EndWhile

    If allCompleted && !IsStageDone(700)
        SetStage(700)
    EndIf
EndFunction

Event OnQuestShutdown()
    CancelTimer(1)
EndEvent
