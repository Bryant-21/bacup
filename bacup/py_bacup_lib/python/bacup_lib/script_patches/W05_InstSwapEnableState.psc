Function EvaluateSwapCriteria()
    Bool criteriaMet = !ORCriteria
    Actor player = Game.GetPlayer()
    Int i = 0
    While i < EnableStates.Length
        Bool thisMet
        If EnableStates[i].SwapAV
            thisMet = player.GetValue(EnableStates[i].SwapAV) >= EnableStates[i].TargetValue
        ElseIf EnableStates[i].SwapQuest
            thisMet = EnableStates[i].SwapQuest.IsCompleted()
        EndIf
        If ORCriteria
            criteriaMet = criteriaMet || thisMet
        Else
            criteriaMet = criteriaMet && thisMet
        EndIf
        i += 1
    EndWhile

    If !criteriaMet && IgnoreIfCriteriaNotMet
        Return
    EndIf

    If criteriaMet == EnableObject
        Enable()
    Else
        Disable()
    EndIf
EndFunction

Event OnInit()
    EvaluateSwapCriteria()
EndEvent

Event OnLoad()
    EvaluateSwapCriteria()
EndEvent
