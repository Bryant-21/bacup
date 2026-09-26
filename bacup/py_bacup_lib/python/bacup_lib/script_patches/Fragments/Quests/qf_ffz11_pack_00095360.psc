B21:QuestTimer Function GetEventTimer()
    Quest owner = Self as Quest
    Return owner as B21:QuestTimer
EndFunction

Bool Function IsEventResolved()
    Return IsStageDone(1000) || IsStageDone(1500)
EndFunction

Function ResetEventObjective(Int aiObjective)
    SetObjectiveDisplayed(aiObjective, False)
    SetObjectiveCompleted(aiObjective, False)
    SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetEventObjectives()
    ResetEventObjective(10)
    ResetEventObjective(20)
    ResetEventObjective(30)
    ResetEventObjective(40)
EndFunction

Function CompleteOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveCompleted(aiObjective, True)
    EndIf
EndFunction

Function FailOpenObjective(Int aiObjective)
    If IsObjectiveDisplayed(aiObjective) && !IsObjectiveCompleted(aiObjective) && !IsObjectiveFailed(aiObjective)
        SetObjectiveFailed(aiObjective, True)
    EndIf
EndFunction

Function RefreshOpenObjectives()
    If !IsObjectiveCompleted(10) && !IsStageDone(200)
        SetObjectiveDisplayed(10, True)
    EndIf
    If !IsObjectiveCompleted(20) && !IsStageDone(300)
        SetObjectiveDisplayed(20, True)
    EndIf
    If !IsObjectiveCompleted(30) && !IsStageDone(400)
        SetObjectiveDisplayed(30, True)
    EndIf
    If IsStageDone(450) && !IsObjectiveCompleted(40)
        SetObjectiveDisplayed(40, True)
    EndIf
EndFunction

Function StartEventTimer()
    ; FO76 ran the activity timer from the server; no converted stage carries the
    ; StartTimer flag, so the hunt stage arms B21:QuestTimer (1200 s) itself.
    B21:QuestTimer eventTimer = GetEventTimer()
    If eventTimer != None && !eventTimer.IsQuestTimerRunning()
        eventTimer.StartQuestTimer()
    EndIf
EndFunction

Function StopEventTimer()
    B21:QuestTimer eventTimer = GetEventTimer()
    If eventTimer != None
        eventTimer.StopQuestTimer()
    EndIf
EndFunction

Function ScheduleEventShutdown(Float afDelay)
    StartTimer(afDelay, 9536)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 9536 && IsRunning()
        Stop()
    ElseIf aiTimerID == 9537
        If IsRunning() && !IsEventResolved() && !TryStartAlphaHunt() && !IsStageDone(1000)
            ; No alpha wolf to hunt, so the pack leaders were the whole objective set.
            SetStage(1000)
        EndIf
    EndIf
EndEvent

Bool Function TryStartAlphaHunt()
    Actor alphaWolf = None
    If Alias_AlphaWolf != None
        alphaWolf = Alias_AlphaWolf.GetActorReference()
    EndIf
    If alphaWolf == None
        Return False
    EndIf
    If alphaWolf.IsDisabled()
        alphaWolf.Enable(False)
    EndIf
    SetObjectiveDisplayed(40, True, True)
    Return True
EndFunction

Function CheckPackLeaders()
    If IsEventResolved() || IsStageDone(450)
        Return
    EndIf
    If IsStageDone(200) && IsStageDone(300) && IsStageDone(400)
        SetStage(450)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    CancelTimer(9536)
    CancelTimer(9537)
    ResetEventObjectives()
    SetObjectiveDisplayed(10, True, True)
    SetObjectiveDisplayed(20, True)
    SetObjectiveDisplayed(30, True)
    StartEventTimer()
EndFunction

Function Fragment_Stage_0200_Item_00()
    CompleteOpenObjective(10)
    CheckPackLeaders()
EndFunction

Function Fragment_Stage_0300_Item_00()
    CompleteOpenObjective(20)
    CheckPackLeaders()
EndFunction

Function Fragment_Stage_0400_Item_00()
    CompleteOpenObjective(30)
    CheckPackLeaders()
EndFunction

Function Fragment_Stage_0450_Item_00()
    If IsEventResolved()
        Return
    EndIf
    CompleteOpenObjective(10)
    CompleteOpenObjective(20)
    CompleteOpenObjective(30)
    If !TryStartAlphaHunt()
        ; The alpha is a created reference; re-check once before ruling it out.
        StartTimer(5.0, 9537)
    EndIf
EndFunction

Function Fragment_Stage_0500_Item_00()
    ; Late-join cutoff: DefaultEventQuest reads the stage itself, so the fragment
    ; only repairs objectives for a player who joined before the cutoff.
    If IsEventResolved()
        Return
    EndIf
    RefreshOpenObjectives()
EndFunction

Function Fragment_Stage_1000_Item_00()
    CancelTimer(9537)
    StopEventTimer()
    CompleteOpenObjective(10)
    CompleteOpenObjective(20)
    CompleteOpenObjective(30)
    CompleteOpenObjective(40)
    ScheduleEventShutdown(60.0)
EndFunction

Function Fragment_Stage_1500_Item_00()
    If IsStageDone(1000)
        Return
    EndIf
    CancelTimer(9537)
    StopEventTimer()
    FailOpenObjective(10)
    FailOpenObjective(20)
    FailOpenObjective(30)
    FailOpenObjective(40)
    ScheduleEventShutdown(20.0)
EndFunction
