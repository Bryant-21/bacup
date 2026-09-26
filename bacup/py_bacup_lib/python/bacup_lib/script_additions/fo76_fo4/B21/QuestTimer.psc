Scriptname B21:QuestTimer Extends Quest
{FO4 substitute for the FO76 quest timer.

FO76 quests carry QuestTimerLengthMax, start that timer when a stage flagged
StartTimer is set, run stages flagged TimerEnd when it expires, and let scripts
react in OnQuestTimerEnd. FO4 drops all of it, so the converter attaches this
script with the timer length and both flagged stage lists. Consumers register
for QuestTimerEnded instead of OnQuestTimerEnd.}

GlobalVariable Property TimerLength Auto Const
{The quest's QuestTimerLengthMax global, in seconds.}

Int[] Property StartTimerStages Auto Const
{Stages whose FO76 stage flags include StartTimer.}

Int[] Property TimerEndStages Auto Const
{Stages whose FO76 stage flags include TimerEnd.}

CustomEvent QuestTimerEnded

Float TimerRemaining
Bool TimerRunning

Event OnQuestInit()
    TimerRemaining = 0.0
    TimerRunning = False
    ; FO76 starts the quest timer server-side when no stage carries StartTimer.
    ; Without this FFZ17_TeaTime's TimerEnd stages 600/700 could never run and
    ; the event would sit open forever. A later StartQuestTimer call re-arms it.
    If StartTimerStages == None || StartTimerStages.Length == 0
        If TimerEndStages != None && TimerEndStages.Length > 0
            StartQuestTimer()
        EndIf
    EndIf
EndEvent

Event OnStageSet(Int auiStageID, Int auiItemID)
    If StartTimerStages != None && StartTimerStages.Find(auiStageID) >= 0
        StartQuestTimer()
    EndIf
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 7801 || !TimerRunning
        Return
    EndIf
    TimerRemaining -= 1.0
    If TimerRemaining > 0.0
        StartTimer(1.0, 7801)
        Return
    EndIf
    TimerRemaining = 0.0
    TimerRunning = False
    FinishQuestTimer()
EndEvent

Event OnQuestShutdown()
    StopQuestTimer()
EndEvent

Function StartQuestTimer(Float afSeconds = -1.0)
    If afSeconds < 0.0
        If TimerLength == None
            Return
        EndIf
        afSeconds = TimerLength.GetValue()
    EndIf
    CancelTimer(7801)
    TimerRemaining = afSeconds
    TimerRunning = afSeconds > 0.0
    If TimerRunning
        ; A one-second countdown keeps the remaining time in the save and pauses with the game.
        StartTimer(1.0, 7801)
    Else
        FinishQuestTimer()
    EndIf
EndFunction

Function FinishQuestTimer()
    RunTimerEndStages()
    SendCustomEvent("QuestTimerEnded")
EndFunction

Function RunTimerEndStages()
    Int index = 0
    While TimerEndStages != None && index < TimerEndStages.Length
        Int stage = TimerEndStages[index]
        ; A TimerEnd stage can route the outcome itself — MTR10_Battle 300 reads
        ; which stages are done and then sets 400 (fail) or 500 (stop). Running
        ; the remaining stages after that would overwrite its decision, so stop
        ; as soon as one of them has moved the quest on.
        If IsStopping() || IsStopped() || IsCompleted() || GetStageDone(stage)
            Return
        EndIf
        SetStage(stage)
        index += 1
    EndWhile
EndFunction

Function StopQuestTimer()
    CancelTimer(7801)
    TimerRunning = False
    TimerRemaining = 0.0
EndFunction

Function EnsureQuestTimerRemaining(Float afSeconds)
    If TimerRunning && TimerRemaining < afSeconds
        TimerRemaining = afSeconds
    EndIf
EndFunction

Bool Function IsQuestTimerRunning()
    Return TimerRunning
EndFunction

Float Function GetQuestTimerRemaining()
    Return TimerRemaining
EndFunction
