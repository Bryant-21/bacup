B21:QuestVariables Function GetEventVariables()
    Quest owner = Self as Quest
    Return owner as B21:QuestVariables
EndFunction

B21:QuestTimer Function GetEventTimer()
    Quest owner = Self as Quest
    Return owner as B21:QuestTimer
EndFunction

Bool Function IsEventResolved()
    Return IsStageDone(500) || IsStageDone(600) || IsStageDone(8900)
EndFunction

Function ResetEventObjective(Int aiObjective)
    SetObjectiveDisplayed(aiObjective, False)
    SetObjectiveCompleted(aiObjective, False)
    SetObjectiveFailed(aiObjective, False)
EndFunction

Function ResetEventObjectives()
    ResetEventObjective(300)
    ResetEventObjective(400)
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

Int Function CountSupervisorsDown()
    Int downed = 0
    If IsStageDone(310)
        downed += 1
    EndIf
    If IsStageDone(320)
        downed += 1
    EndIf
    If IsStageDone(330)
        downed += 1
    EndIf
    Return downed
EndFunction

Function PublishSupervisorCount()
    ; Objective 300 shows the count through the converter's quest-variable global.
    B21:QuestVariables eventVariables = GetEventVariables()
    If eventVariables != None
        eventVariables.SetVariable("CodesAcquired", CountSupervisorsDown() as Float)
    EndIf
EndFunction

Function SetFarmhandsHostile(Bool abHostile)
    If pReaperBotFaction == None || pPlayerFaction == None
        Return
    EndIf
    If abHostile
        pReaperBotFaction.SetEnemy(pPlayerFaction)
    Else
        ; Resetting the targeting parameters is what removes humans from the target list.
        pReaperBotFaction.SetAlly(pPlayerFaction)
    EndIf
EndFunction

Function StopEventTimer()
    B21:QuestTimer eventTimer = GetEventTimer()
    If eventTimer != None
        eventTimer.StopQuestTimer()
    EndIf
EndFunction

Function ScheduleEventShutdown(Float afDelay)
    StartTimer(afDelay, 65005)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 65005 && IsRunning()
        Stop()
    EndIf
EndEvent

Function CheckSupervisors()
    If IsEventResolved() || IsStageDone(400)
        Return
    EndIf
    If CountSupervisorsDown() >= 3
        SetStage(400)
    EndIf
EndFunction

Function Fragment_Stage_0001_Item_00()
    CancelTimer(65005)
    ResetEventObjectives()
    PublishSupervisorCount()
    SetFarmhandsHostile(True)
    SetObjectiveDisplayed(300, True, True)
    If !IsStageDone(100)
        SetStage(100)
    EndIf
EndFunction

Function Fragment_Stage_0100_Item_00()
    ; StartTimer stage: B21:QuestTimer arms the 1800 s activity timer from here.
    If IsEventResolved()
        Return
    EndIf
    PublishSupervisorCount()
    SetObjectiveDisplayed(300, True, True)
EndFunction

Function Fragment_Stage_0310_Item_00()
    PublishSupervisorCount()
    CheckSupervisors()
EndFunction

Function Fragment_Stage_0320_Item_00()
    PublishSupervisorCount()
    CheckSupervisors()
EndFunction

Function Fragment_Stage_0330_Item_00()
    PublishSupervisorCount()
    CheckSupervisors()
EndFunction

Function Fragment_Stage_0400_Item_00()
    If IsEventResolved()
        Return
    EndIf
    PublishSupervisorCount()
    CompleteOpenObjective(300)
    SetObjectiveDisplayed(400, True, True)
EndFunction

Function Fragment_Stage_0500_Item_00()
    StopEventTimer()
    CompleteOpenObjective(300)
    CompleteOpenObjective(400)
    SetFarmhandsHostile(False)
    If !IsStageDone(600)
        SetStage(600)
    EndIf
EndFunction

Function Fragment_Stage_0600_Item_00()
    StopEventTimer()
    ScheduleEventShutdown(60.0)
EndFunction

Function Fragment_Stage_8900_Item_00()
    If IsStageDone(500)
        Return
    EndIf
    StopEventTimer()
    FailOpenObjective(300)
    FailOpenObjective(400)
    ScheduleEventShutdown(20.0)
EndFunction

Function Fragment_Stage_9000_Item_00()
    ; RunOnStop stage.
    CancelTimer(65005)
    StopEventTimer()
EndFunction
