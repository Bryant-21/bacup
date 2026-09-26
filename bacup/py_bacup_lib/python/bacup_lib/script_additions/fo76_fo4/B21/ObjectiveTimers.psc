Scriptname B21:ObjectiveTimers Extends Quest
{FO4 substitute for FO76 objective timers.

An FO76 objective flagged UsesTimer counts down QuestObjectiveTimer seconds while
it is displayed and sets its StageToSet when the countdown ends. FO4 drops all
three fields, so the converter attaches this script with one parallel row per
timed objective.}

Int[] Property TimedObjectives Auto Const
{Objective indices flagged UsesTimer.}

GlobalVariable[] Property TimerLengths Auto Const
{QuestObjectiveTimer globals, in seconds, parallel to TimedObjectives.}

Int[] Property ExpiryStages Auto Const
{Objective StageToSet values, parallel to TimedObjectives.}

Float[] Remaining
Bool[] Armed

Event OnQuestInit()
    Remaining = None
    Armed = None
    StartTimer(1.0, 7901)
EndEvent

Event OnTimer(Int aiTimerID)
    If aiTimerID != 7901
        Return
    EndIf
    If IsStopping() || IsStopped() || IsCompleted()
        Return
    EndIf
    TickObjectiveTimers()
    StartTimer(1.0, 7901)
EndEvent

Event OnQuestShutdown()
    CancelTimer(7901)
EndEvent

Bool Function IsValid()
    If TimedObjectives == None || TimerLengths == None || ExpiryStages == None
        Return False
    EndIf
    Return TimerLengths.Length == TimedObjectives.Length && ExpiryStages.Length == TimedObjectives.Length
EndFunction

Function TickObjectiveTimers()
    If !IsValid()
        Return
    EndIf
    If Armed == None || Armed.Length != TimedObjectives.Length
        Armed = New Bool[TimedObjectives.Length]
        Remaining = New Float[TimedObjectives.Length]
    EndIf

    Int row = 0
    While row < TimedObjectives.Length
        Int objective = TimedObjectives[row]
        Bool active = IsObjectiveDisplayed(objective) && !IsObjectiveCompleted(objective) && !IsObjectiveFailed(objective)
        If !active
            Armed[row] = False
        ElseIf !Armed[row]
            If TimerLengths[row] != None && TimerLengths[row].GetValue() > 0.0
                Armed[row] = True
                Remaining[row] = TimerLengths[row].GetValue()
            EndIf
        Else
            Remaining[row] = Remaining[row] - 1.0
            If Remaining[row] <= 0.0
                Armed[row] = False
                Int stage = ExpiryStages[row]
                ; Repeatable events (AllowRepeatedStages) expire each round into
                ; a stage that an earlier round already set. The engine ignores
                ; the repeat on other quests.
                If stage >= 0 && (!GetStageDone(stage) || GetStage() != stage)
                    SetStage(stage)
                EndIf
            EndIf
        EndIf
        row += 1
    EndWhile
EndFunction

Float Function GetObjectiveTimeRemaining(Int aiObjective)
    If !IsValid() || Armed == None
        Return 0.0
    EndIf
    Int row = TimedObjectives.Find(aiObjective)
    If row < 0 || !Armed[row]
        Return 0.0
    EndIf
    Return Remaining[row]
EndFunction
