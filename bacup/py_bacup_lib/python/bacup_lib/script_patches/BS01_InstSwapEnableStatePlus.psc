Event OnInit()
    BeginMonitoring()
EndEvent

Event OnCellAttach()
    BeginMonitoring()
EndEvent

Event OnCellDetach()
    CancelTimer(1)
EndEvent

Event OnTimer(Int aiTimerID)
    if aiTimerID == 1
        ApplyEnableState()
        StartTimer(1.0, 1)
    endif
EndEvent

Function BeginMonitoring()
    CancelTimer(1)
    ApplyEnableState()
    StartTimer(1.0, 1)
EndFunction

Function ApplyEnableState()
    bool shouldEnable = EvaluateCriteria()
    if !EnableObject
        shouldEnable = !shouldEnable
    endif

    if shouldEnable && IsDisabled()
        EnableNoWait()
    elseif !shouldEnable && !IsDisabled()
        DisableNoWait()
    endif
EndFunction

Bool Function EvaluateCriteria()
    if EnableStates == None || EnableStates.Length == 0
        return False
    endif

    if ORCriteria
        int i = 0
        while i < EnableStates.Length
            if CriterionMatches(EnableStates[i])
                return True
            endif
            i += 1
        endwhile
        return False
    endif

    int i = 0
    while i < EnableStates.Length
        if !CriterionMatches(EnableStates[i])
            return False
        endif
        i += 1
    endwhile
    return True
EndFunction

Bool Function CriterionMatches(EnableCriteria akCriteria)
    bool hasQuest = akCriteria.SwapQuest != None
    bool hasActorValue = akCriteria.SwapAV != None
    bool questMatches = False
    bool actorValueMatches = False

    if hasQuest && (!hasActorValue || akCriteria.CheckBothQuestAndAV)
        questMatches = akCriteria.SwapQuest.IsCompleted()
    endif

    if hasActorValue
        float currentValue = Game.GetPlayer().GetValue(akCriteria.SwapAV)
        if currentValue < akCriteria.TargetValue
            actorValueMatches = akCriteria.MaintainStateOnLessThanTargetValue
        elseif currentValue == akCriteria.TargetValue
            actorValueMatches = True
        else
            actorValueMatches = akCriteria.MaintainStateOnGreaterThanTargetValue
        endif
    endif

    if hasQuest && hasActorValue
        if akCriteria.CheckBothQuestAndAV
            return questMatches && actorValueMatches
        endif
        return actorValueMatches
    elseif hasQuest
        return questMatches
    elseif hasActorValue
        return actorValueMatches
    endif

    return False
EndFunction
