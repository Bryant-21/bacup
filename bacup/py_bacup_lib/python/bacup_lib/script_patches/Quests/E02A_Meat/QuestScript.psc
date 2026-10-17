; B21:QuestTimer runs E02A_Meat_BBQ_QuestTimer from 150 to 400; this script marks its halfway and last-minute
; stages and gives the player the personal contribution quest (grill, feed pile, woodpile).
Event OnStageSet(Int auiStageID, Int auiItemID)
    If auiStageID == 150
        StartPersonalQuest()
        If CSDuckMusic != None
            CSDuckMusic.Push()
        EndIf
        If E02A_Meat_BBQ_QuestTimer != None
            Float total = E02A_Meat_BBQ_QuestTimer.GetValue()
            If total > 0.0
                StartTimer(total / 2.0, TimerID_Halftime)
            EndIf
            If total > 60.0
                StartTimer(total - 60.0, TimerID_OneMinLeft)
            EndIf
        EndIf
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If !IsRunning() || IsStageDone(400)
        Return
    EndIf
    If aiTimerID == TimerID_Halftime && !IsStageDone(Stage_Halftime)
        SetStage(Stage_Halftime)
    ElseIf aiTimerID == TimerID_OneMinLeft && !IsStageDone(Stage_OneMinLeft)
        SetStage(Stage_OneMinLeft)
    EndIf
EndEvent

Function StartPersonalQuest()
    If E02A_Meat_BBQ_Personal == None
        Return
    EndIf
    If E02A_Meat_BBQ_Personal.IsRunning()
        E02A_Meat_BBQ_Personal.Stop()
    EndIf
    E02A_Meat_BBQ_Personal.Reset()
    If E02A_Meat_BBQ_Personal.Start()
        ReferenceAlias player = E02A_Meat_BBQ_Personal.GetAlias(115) as ReferenceAlias
        If player != None
            player.ForceRefTo(Game.GetPlayer())
        EndIf
    EndIf
EndFunction

Event OnQuestShutdown()
    CancelTimer(TimerID_Halftime)
    CancelTimer(TimerID_OneMinLeft)
    If CSDuckMusic != None
        CSDuckMusic.Remove()
    EndIf
    If E02A_Meat_BBQ_Personal != None && E02A_Meat_BBQ_Personal.IsRunning()
        E02A_Meat_BBQ_Personal.Stop()
    EndIf
EndEvent
