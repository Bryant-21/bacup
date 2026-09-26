; Every restart quest is RunOnce and has no stage that stops it, so the Story
; Manager starts it on the first qualifying player connect only. Later loads are
; re-evaluated through the player's load event instead.
Event OnQuestInit()
    RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
    ResumeQuestline()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    ResumeQuestline()
EndEvent

; Resume only: a line the player's save never recorded as started is left alone,
; so neither the fallback entry nor the alternate-quest proof of a start is used.
Function ResumeQuestline()
    Actor player = Game.GetPlayer()
    If player == None || QuestlineStartedValue == None || Questline == None
        Return
    EndIf
    If player.GetValue(QuestlineStartedValue) <= 0
        Return
    EndIf

    Int index = Questline.Length - 1
    While index >= 0
        QuestRestartData checkpoint = Questline[index]
        If checkpoint != None && RequiredQuestsCompleted(checkpoint)
            Quest target = checkpoint.TargetQuest
            If target == None
                ; A keyword-only checkpoint names no quest whose state proves it unfinished.
                Return
            ElseIf target.IsCompleted()
                If !checkpoint.ContinueEvalIfTargetQuestComplete
                    Return
                EndIf
            ElseIf target.IsRunning()
                Return
            Else
                RestartCheckpoint(checkpoint)
                Return
            EndIf
        EndIf
        index -= 1
    EndWhile
EndFunction

Bool Function RequiredQuestsCompleted(QuestRestartData akCheckpoint)
    Return QuestCompletedOrUnbound(akCheckpoint.RequiredQuest) && QuestCompletedOrUnbound(akCheckpoint.RequiredQuest02) && QuestCompletedOrUnbound(akCheckpoint.RequiredQuest03)
EndFunction

Bool Function QuestCompletedOrUnbound(Quest akQuest)
    Return akQuest == None || akQuest.IsCompleted()
EndFunction

Function RestartCheckpoint(QuestRestartData akCheckpoint)
    Bool started
    If akCheckpoint.KeywordToRestart != None
        started = akCheckpoint.KeywordToRestart.SendStoryEventAndWait(akCheckpoint.LocToSend, None, None, akCheckpoint.Value1, akCheckpoint.Value2)
    Else
        started = akCheckpoint.TargetQuest.Start()
    EndIf
    Debug.Trace("[" + DejaChannel + "] " + Self as String + " DefaultQuestlineRestartScript| resumed " + akCheckpoint.TargetQuest as String + " started=" + started as String, 0)
EndFunction
